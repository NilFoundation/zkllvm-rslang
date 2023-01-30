//! Errors emitted by `rustc_hir_typeck`.
use rustc_errors::{AddToDiagnostic, Applicability, Diagnostic, MultiSpan, SubdiagnosticMessage};
use rustc_macros::{Diagnostic, Subdiagnostic};
use rustc_middle::ty::Ty;
use rustc_span::{symbol::Ident, Span};

#[derive(Diagnostic)]
#[diag(hir_typeck_field_multiply_specified_in_initializer, code = "E0062")]
pub struct FieldMultiplySpecifiedInInitializer {
    #[primary_span]
    #[label]
    pub span: Span,
    #[label(previous_use_label)]
    pub prev_span: Span,
    pub ident: Ident,
}

#[derive(Diagnostic)]
#[diag(hir_typeck_return_stmt_outside_of_fn_body, code = "E0572")]
pub struct ReturnStmtOutsideOfFnBody {
    #[primary_span]
    pub span: Span,
    #[label(encl_body_label)]
    pub encl_body_span: Option<Span>,
    #[label(encl_fn_label)]
    pub encl_fn_span: Option<Span>,
}

#[derive(Diagnostic)]
#[diag(hir_typeck_yield_expr_outside_of_generator, code = "E0627")]
pub struct YieldExprOutsideOfGenerator {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(hir_typeck_struct_expr_non_exhaustive, code = "E0639")]
pub struct StructExprNonExhaustive {
    #[primary_span]
    pub span: Span,
    pub what: &'static str,
}

#[derive(Diagnostic)]
#[diag(hir_typeck_method_call_on_unknown_type, code = "E0699")]
pub struct MethodCallOnUnknownType {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(hir_typeck_functional_record_update_on_non_struct, code = "E0436")]
pub struct FunctionalRecordUpdateOnNonStruct {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(hir_typeck_address_of_temporary_taken, code = "E0745")]
pub struct AddressOfTemporaryTaken {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Subdiagnostic)]
pub enum AddReturnTypeSuggestion {
    #[suggestion(
        hir_typeck_add_return_type_add,
        code = "-> {found} ",
        applicability = "machine-applicable"
    )]
    Add {
        #[primary_span]
        span: Span,
        found: String,
    },
    #[suggestion(
        hir_typeck_add_return_type_missing_here,
        code = "-> _ ",
        applicability = "has-placeholders"
    )]
    MissingHere {
        #[primary_span]
        span: Span,
    },
}

#[derive(Subdiagnostic)]
pub enum ExpectedReturnTypeLabel<'tcx> {
    #[label(hir_typeck_expected_default_return_type)]
    Unit {
        #[primary_span]
        span: Span,
    },
    #[label(hir_typeck_expected_return_type)]
    Other {
        #[primary_span]
        span: Span,
        expected: Ty<'tcx>,
    },
}

#[derive(Diagnostic)]
#[diag(hir_typeck_missing_parentheses_in_range, code = "E0689")]
pub struct MissingParentheseInRange {
    #[primary_span]
    #[label(hir_typeck_missing_parentheses_in_range)]
    pub span: Span,
    pub ty_str: String,
    pub method_name: String,
    #[subdiagnostic]
    pub add_missing_parentheses: Option<AddMissingParenthesesInRange>,
}

#[derive(Subdiagnostic)]
#[multipart_suggestion(
    hir_typeck_add_missing_parentheses_in_range,
    style = "verbose",
    applicability = "maybe-incorrect"
)]
pub struct AddMissingParenthesesInRange {
    pub func_name: String,
    #[suggestion_part(code = "(")]
    pub left: Span,
    #[suggestion_part(code = ")")]
    pub right: Span,
}

#[derive(Diagnostic)]
#[diag(hir_typeck_op_trait_generic_params)]
pub struct OpMethodGenericParams {
    #[primary_span]
    pub span: Span,
    pub method_name: String,
}

pub struct TypeMismatchFruTypo {
    /// Span of the LHS of the range
    pub expr_span: Span,
    /// Span of the `..RHS` part of the range
    pub fru_span: Span,
    /// Rendered expression of the RHS of the range
    pub expr: Option<String>,
}

impl AddToDiagnostic for TypeMismatchFruTypo {
    fn add_to_diagnostic_with<F>(self, diag: &mut Diagnostic, _: F)
    where
        F: Fn(&mut Diagnostic, SubdiagnosticMessage) -> SubdiagnosticMessage,
    {
        diag.set_arg("expr", self.expr.as_deref().unwrap_or("NONE"));

        // Only explain that `a ..b` is a range if it's split up
        if self.expr_span.between(self.fru_span).is_empty() {
            diag.span_note(
                self.expr_span.to(self.fru_span),
                rustc_errors::fluent::hir_typeck_fru_note,
            );
        } else {
            let mut multispan: MultiSpan = vec![self.expr_span, self.fru_span].into();
            multispan.push_span_label(self.expr_span, rustc_errors::fluent::hir_typeck_fru_expr);
            multispan.push_span_label(self.fru_span, rustc_errors::fluent::hir_typeck_fru_expr2);
            diag.span_note(multispan, rustc_errors::fluent::hir_typeck_fru_note);
        }

        diag.span_suggestion(
            self.expr_span.shrink_to_hi(),
            rustc_errors::fluent::hir_typeck_fru_suggestion,
            ", ",
            Applicability::MaybeIncorrect,
        );
    }
}
