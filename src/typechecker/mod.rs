pub mod error;
pub use error::TypeError;

pub mod ty;
pub use ty::{FnSig, Ty};

pub(crate) mod env;
pub(crate) mod infer;

use crate::ast::Ast;
use crate::lexer::token::Span;
use env::StructInfo;
use std::collections::HashMap;

/// Opaque result returned by a successful inference pass.
///
/// Internal representation can grow (new fields, region solutions, etc.) without
/// changing the public type — callers access it through the provided accessor methods.
pub struct InferResult {
    #[allow(dead_code)] // used by codegen in PR 31+
    pub(crate) type_map: HashMap<Span, Ty>,
    /// Non-fatal `TypeError::Deferred` diagnostics collected during inference.
    /// These represent constructs that were accepted syntactically but NOT fully
    /// type-checked in phase 1. Surfaced here (rather than silently dropped) so
    /// callers and the driver can warn the user — lowering a `Ty::Error`-typed
    /// node to codegen without this signal would violate pillar 1.
    pub deferred: Vec<TypeError>,
    pub(crate) struct_defs: HashMap<String, StructInfo>,
    #[allow(dead_code)] // reserved for future method/associated-fn lookup in codegen
    pub(crate) impl_sigs: HashMap<String, HashMap<String, (FnSig, Span)>>,
    // PHASE2: pub(crate) region_solution: RegionSolution,
}

impl InferResult {
    /// Look up the inferred type for the expression at the given source span.
    /// Returns `None` for spans not recorded (e.g. spans not yet visited).
    #[allow(dead_code)] // used by codegen in PR 31+
    pub fn type_of(&self, span: Span) -> Option<&Ty> {
        self.type_map.get(&span)
    }

    /// True if any deferred (unverified) constructs were encountered.
    /// Codegen should refuse to lower nodes typed `Ty::Error`.
    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
    }

    /// GEP index for `field_name` in `type_name`, in source declaration order.
    pub fn struct_field_index(&self, type_name: &str, field_name: &str) -> Option<usize> {
        self.struct_defs.get(type_name)?.field_order.iter().position(|f| f == field_name)
    }

    /// Resolved `Ty` of `field_name` inside `type_name`.
    pub fn struct_field_type(&self, type_name: &str, field_name: &str) -> Option<&Ty> {
        self.struct_defs.get(type_name)?.fields.get(field_name)
    }

    /// Field names in source declaration order.
    pub fn struct_field_names(&self, type_name: &str) -> Option<&[String]> {
        self.struct_defs.get(type_name).map(|info| info.field_order.as_slice())
    }
}

/// Run type inference and checking over a parsed AST.
///
/// Returns `Ok(InferResult)` if no fatal type errors are found.
/// `InferResult::deferred` carries non-fatal `TypeError::Deferred` entries for
/// constructs that were not fully verified — callers should warn on these.
/// Returns `Err(errors)` (fatal errors only) when the program is ill-typed.
pub fn infer(ast: &Ast<'_>) -> Result<InferResult, Vec<TypeError>> {
    infer::run(ast)
}
