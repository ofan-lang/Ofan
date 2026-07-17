use crate::ast::{Block, Expr, Stmt, UnaryOp};
use crate::lexer::token::Span;
use crate::typechecker::env::InferCtx;
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::Ty;

// ─── §18 self access-mode inference ──────────────────────────────────────────

/// Infer the access mode for a bare `self` receiver by scanning the method body.
/// Returns `Ty::Named` (consuming), `Ty::Ref { mutable: true }` (mutating),
/// or `Ty::Ref { mutable: false }` (read-only). Emits `SelfAccessAmbiguity` if
/// consuming and non-consuming uses coexist in the same body.
pub(super) fn infer_self_access_mode(
    type_name: &str,
    body: &Block<'_>,
    fn_name: &str,
    ctx: &mut InferCtx,
) -> Ty {
    let scan = scan_self_usage(body);

    // §18: consuming + any borrowing use (method receiver OR field mutation) is ambiguous.
    // Mutating field access (`self.x = ...`) is a mutable-borrow-level use and conflicts
    // with a consuming move of self in the same body.
    let borrowing = scan.non_consuming.or(scan.mutating);
    if let (Some(c), Some(other)) = (scan.consuming, borrowing) {
        ctx.error(TypeError::SelfAccessAmbiguity {
            fn_name: fn_name.to_string(),
            consuming_span: c,
            other_span: other,
        });
        return Ty::Error;
    }

    let inner = Ty::Named(type_name.to_string());
    match (scan.consuming.is_some(), scan.mutating.is_some()) {
        (true, _) => inner,
        (false, true) => Ty::Ref { mutable: true, region: None, inner: Box::new(inner) },
        (false, false) => Ty::Ref { mutable: false, region: None, inner: Box::new(inner) },
    }
}

struct SelfUsageScan {
    consuming: Option<Span>,     // first consuming use (self moved out)
    mutating: Option<Span>,      // first mutating use (self.field = ..., &mut self)
    non_consuming: Option<Span>, // first non-consuming use (self as method receiver, read)
}

/// Pure AST scan — no type information used. Walks the full block recursively,
/// classifying each occurrence of `self` by its syntactic usage context.
fn scan_self_usage(block: &Block<'_>) -> SelfUsageScan {
    let mut scan = SelfUsageScan { consuming: None, mutating: None, non_consuming: None };
    scan_block(block, &mut scan);
    scan
}

fn scan_block(block: &Block<'_>, scan: &mut SelfUsageScan) {
    for stmt in &block.stmts {
        scan_stmt(stmt, scan);
    }
    if let Some(tail) = &block.tail {
        // Tail expression: `self` as tail means it is consumed (returned by value).
        if is_self_ident(tail) {
            set_consuming(tail.span(), scan);
        } else {
            scan_expr(tail, scan);
        }
    }
}

fn scan_stmt(stmt: &Stmt<'_>, scan: &mut SelfUsageScan) {
    match stmt {
        Stmt::Let { init, .. } => {
            if is_self_ident(init) {
                set_consuming(init.span(), scan);
            } else {
                scan_expr(init, scan);
            }
        }
        Stmt::Const { init, .. } => scan_expr(init, scan),
        Stmt::Return { value: Some(v), .. } => {
            if is_self_ident(v) {
                set_consuming(v.span(), scan);
            } else {
                scan_expr(v, scan);
            }
        }
        Stmt::Return { value: None, .. } | Stmt::Continue { .. } | Stmt::Break { value: None, .. } => {}
        Stmt::Break { value: Some(v), .. } => scan_expr(v, scan),
        Stmt::Assign { target, value, .. } => {
            // `self.field = x` → mutating; `self = x` → non-self (lvalue assignment, unusual)
            if let Expr::Field { object, .. } = target.as_ref() {
                if is_self_ident(object) {
                    set_mutating(object.span(), scan);
                } else {
                    scan_expr(object, scan);
                }
            } else {
                scan_expr(target, scan);
            }
            if is_self_ident(value) {
                set_consuming(value.span(), scan);
            } else {
                scan_expr(value, scan);
            }
        }
        Stmt::Expr { expr, .. } => scan_expr(expr, scan),
    }
}

fn scan_expr(expr: &Expr<'_>, scan: &mut SelfUsageScan) {
    match expr {
        Expr::Ident("self", span) => set_non_consuming(*span, scan),
        Expr::Ident(_, _) | Expr::Literal(_, _) => {}

        Expr::Unary { op: UnaryOp::BorrowMut, expr: inner, .. } => {
            // `&mut self` is a mutating use.
            if is_self_ident(inner) {
                set_mutating(inner.span(), scan);
            } else {
                scan_expr(inner, scan);
            }
        }
        Expr::Unary { expr: inner, .. } => scan_expr(inner, scan),

        Expr::Binary { left, right, .. } => {
            scan_expr(left, scan);
            scan_expr(right, scan);
        }

        // Free function call: `self` as an argument is consumed (moved into the function).
        Expr::Call { callee, args, .. } => {
            scan_expr(callee, scan);
            for arg in args {
                if is_self_ident(arg) {
                    set_consuming(arg.span(), scan);
                } else {
                    scan_expr(arg, scan);
                }
            }
        }

        // Method call: `self` as the *object* receiver is non-consuming (consuming methods
        // require `move self`). `self` appearing in the *argument list* is consuming.
        Expr::MethodCall { object, args, .. } => {
            if is_self_ident(object) {
                set_non_consuming(object.span(), scan);
            } else {
                scan_expr(object, scan);
            }
            for arg in args {
                if is_self_ident(arg) {
                    set_consuming(arg.span(), scan);
                } else {
                    scan_expr(arg, scan);
                }
            }
        }

        Expr::Field { object, .. } => scan_expr(object, scan),
        Expr::Cast { expr: inner, .. } | Expr::Propagate { expr: inner, .. } => {
            scan_expr(inner, scan);
        }

        Expr::Block(b) => scan_block(b, scan),

        Expr::If { condition, then_block, else_branch, .. } => {
            scan_expr(condition, scan);
            scan_block(then_block, scan);
            if let Some(e) = else_branch { scan_expr(e, scan); }
        }
        Expr::While { condition, body, .. } => {
            scan_expr(condition, scan);
            scan_block(body, scan);
        }
        Expr::Loop { body, .. } => scan_block(body, scan),
        Expr::For { iterable, body, .. } => {
            scan_expr(iterable, scan);
            scan_block(body, scan);
        }
        Expr::Match { subject, arms, .. } => {
            scan_expr(subject, scan);
            for arm in arms {
                scan_expr(&arm.body, scan);
                if let Some(g) = &arm.guard { scan_expr(g, scan); }
            }
        }
    }
}

fn is_self_ident(expr: &Expr<'_>) -> bool {
    matches!(expr, Expr::Ident("self", _))
}

fn set_consuming(span: Span, scan: &mut SelfUsageScan) {
    if scan.consuming.is_none() { scan.consuming = Some(span); }
}
fn set_mutating(span: Span, scan: &mut SelfUsageScan) {
    if scan.mutating.is_none() { scan.mutating = Some(span); }
}
fn set_non_consuming(span: Span, scan: &mut SelfUsageScan) {
    if scan.non_consuming.is_none() { scan.non_consuming = Some(span); }
}
