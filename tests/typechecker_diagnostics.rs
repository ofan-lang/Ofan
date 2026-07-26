use ofan::{
    lexer::Lexer,
    parser::Parser,
    typechecker::{self, TypeError},
};

fn type_errors(src: &str) -> Vec<TypeError> {
    let tokens = Lexer::new(src).lex().expect("lex failed");
    let ast = Parser::new(tokens).parse().expect("parse failed");
    match typechecker::infer(&ast) {
        Err(errs) => errs,
        Ok(_) => panic!("expected type errors but typechecker returned Ok"),
    }
}

fn type_check_ok(src: &str) {
    let tokens = Lexer::new(src).lex().expect("lex failed");
    let ast = Parser::new(tokens).parse().expect("parse failed");
    if let Err(errs) = typechecker::infer(&ast) {
        panic!("expected no errors but got: {errs:?}");
    }
}

// ── FieldNotFound ─────────────────────────────────────────────────────────────

#[test]
fn diag_field_not_found_with_available() {
    let errors = type_errors(
        "struct Point { x: f64, y: f64 } fn f(p: Point) -> f64 { p.z }",
    );
    insta::assert_snapshot!(errors[0].to_string());
}

#[test]
fn diag_field_not_found_type_has_no_fields() {
    let errors = type_errors(
        "struct Empty {} fn f(e: Empty) -> i32 { e.x }",
    );
    insta::assert_snapshot!(errors[0].to_string());
}

// ── MissingStructFields ───────────────────────────────────────────────────────

#[test]
fn diag_missing_struct_fields() {
    let errors = type_errors(
        "struct Point { x: f64, y: f64 } fn f() { let _ = Point { x = 1.0 }; }",
    );
    insta::assert_snapshot!(errors[0].to_string());
}

// ── MethodNotFound ────────────────────────────────────────────────────────────

#[test]
fn diag_method_not_found_sorted_available() {
    // `zebra` and `alpha` exist; `missing` does not. Available list must sort alphabetically.
    let errors = type_errors(
        "impl Foo { fn zebra(self) {} fn alpha(self) {} fn bad(self) { self.missing(); } }",
    );
    insta::assert_snapshot!(errors[0].to_string());
}

// ── SelfAccessAmbiguity ───────────────────────────────────────────────────────

#[test]
fn diag_self_access_ambiguity() {
    let errors = type_errors(
        "fn take(x: Foo) {} impl Foo { fn peek(self) {} fn bad(self) { take(self); self.peek(); } }",
    );
    let e = errors
        .iter()
        .find(|e| matches!(e, TypeError::SelfAccessAmbiguity { .. }))
        .expect("SelfAccessAmbiguity not in error list");
    insta::assert_snapshot!(e.to_string());
}

// ── ConsumeViaRef ─────────────────────────────────────────────────────────────

#[test]
fn diag_consume_via_ref() {
    let errors = type_errors(
        "impl Foo { fn consume(move self) {} fn caller(self) { self.consume(); } }",
    );
    insta::assert_snapshot!(errors[0].to_string());
}

// ── DuplicateMethod ───────────────────────────────────────────────────────────

#[test]
fn diag_duplicate_method() {
    let errors = type_errors(
        "impl Foo { fn bar(self) {} } impl Foo { fn bar(self) {} }",
    );
    insta::assert_snapshot!(errors[0].to_string());
}

// ── DuplicateFn ──────────────────────────────────────────────────────────────

#[test]
fn diag_duplicate_fn() {
    let errors = type_errors("fn foo() {} fn foo() {}");
    insta::assert_snapshot!(errors[0].to_string());
}

// ── Mismatch ─────────────────────────────────────────────────────────────────

#[test]
fn diag_mismatch_let_annotation() {
    // `let x: bool = 5` — annotation says bool, initializer is i32.
    let errors = type_errors("fn bad() { let x: bool = 5; let _ = x; }");
    insta::assert_snapshot!(errors[0].to_string());
}

// ── FieldWriteViaSharedRef ────────────────────────────────────────────────────

#[test]
fn diag_field_write_via_shared_ref() {
    let errors = type_errors(
        "struct Point { x: f64 } fn f(r: &Point) { r.x = 1.0; }",
    );
    insta::assert_snapshot!(errors[0].to_string());
}

// ── IntegerOutOfRange ─────────────────────────────────────────────────────────

#[test]
fn diag_integer_out_of_range() {
    // Bare positive overflow — 9999999999 > i32::MAX.
    let errors = type_errors("fn f() -> i32 { let x = 9999999999; x }");
    insta::assert_snapshot!(errors[0].to_string());
}

#[test]
fn ok_i32_min_literal() {
    // -2147483648 = i32::MIN — valid i32 written with explicit negation. Must not error.
    type_check_ok("fn f() -> i32 { -2147483648 }");
}

#[test]
fn diag_integer_out_of_range_neg() {
    // -2147483649 — out of i32 range even with negation; no valid i32 representation.
    let errors = type_errors("fn f() -> i32 { -2147483649 }");
    insta::assert_snapshot!(errors[0].to_string());
}
