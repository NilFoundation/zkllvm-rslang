use hir::def::CtorKind;
use hir::intravisit::{walk_expr, walk_stmt, Visitor};
use rustc_data_structures::fx::FxIndexSet;
use rustc_errors::{Applicability, Diagnostic};
use rustc_hir as hir;
use rustc_middle::traits::{
    IfExpressionCause, MatchExpressionArmCause, ObligationCause, ObligationCauseCode,
    StatementAsExpression,
};
use rustc_middle::ty::print::with_no_trimmed_paths;
use rustc_middle::ty::{self as ty, Ty, TypeVisitable};
use rustc_span::{sym, BytePos, Span};

use crate::errors::SuggAddLetForLetChains;

use super::TypeErrCtxt;

impl<'tcx> TypeErrCtxt<'_, 'tcx> {
    pub(super) fn suggest_remove_semi_or_return_binding(
        &self,
        err: &mut Diagnostic,
        first_id: Option<hir::HirId>,
        first_ty: Ty<'tcx>,
        first_span: Span,
        second_id: Option<hir::HirId>,
        second_ty: Ty<'tcx>,
        second_span: Span,
    ) {
        let remove_semicolon = [
            (first_id, self.resolve_vars_if_possible(second_ty)),
            (second_id, self.resolve_vars_if_possible(first_ty)),
        ]
        .into_iter()
        .find_map(|(id, ty)| {
            let hir::Node::Block(blk) = self.tcx.hir().get(id?) else { return None };
            self.could_remove_semicolon(blk, ty)
        });
        match remove_semicolon {
            Some((sp, StatementAsExpression::NeedsBoxing)) => {
                err.multipart_suggestion(
                    "consider removing this semicolon and boxing the expressions",
                    vec![
                        (first_span.shrink_to_lo(), "Box::new(".to_string()),
                        (first_span.shrink_to_hi(), ")".to_string()),
                        (second_span.shrink_to_lo(), "Box::new(".to_string()),
                        (second_span.shrink_to_hi(), ")".to_string()),
                        (sp, String::new()),
                    ],
                    Applicability::MachineApplicable,
                );
            }
            Some((sp, StatementAsExpression::CorrectType)) => {
                err.span_suggestion_short(
                    sp,
                    "consider removing this semicolon",
                    "",
                    Applicability::MachineApplicable,
                );
            }
            None => {
                for (id, ty) in [(first_id, second_ty), (second_id, first_ty)] {
                    if let Some(id) = id
                        && let hir::Node::Block(blk) = self.tcx.hir().get(id)
                        && self.consider_returning_binding(blk, ty, err)
                    {
                        break;
                    }
                }
            }
        }
    }

    pub(super) fn suggest_boxing_for_return_impl_trait(
        &self,
        err: &mut Diagnostic,
        return_sp: Span,
        arm_spans: impl Iterator<Item = Span>,
    ) {
        err.multipart_suggestion(
            "you could change the return type to be a boxed trait object",
            vec![
                (return_sp.with_hi(return_sp.lo() + BytePos(4)), "Box<dyn".to_string()),
                (return_sp.shrink_to_hi(), ">".to_string()),
            ],
            Applicability::MaybeIncorrect,
        );
        let sugg = arm_spans
            .flat_map(|sp| {
                [(sp.shrink_to_lo(), "Box::new(".to_string()), (sp.shrink_to_hi(), ")".to_string())]
                    .into_iter()
            })
            .collect::<Vec<_>>();
        err.multipart_suggestion(
            "if you change the return type to expect trait objects, box the returned expressions",
            sugg,
            Applicability::MaybeIncorrect,
        );
    }

    pub(super) fn suggest_tuple_pattern(
        &self,
        cause: &ObligationCause<'tcx>,
        exp_found: &ty::error::ExpectedFound<Ty<'tcx>>,
        diag: &mut Diagnostic,
    ) {
        // Heavily inspired by `FnCtxt::suggest_compatible_variants`, with
        // some modifications due to that being in typeck and this being in infer.
        if let ObligationCauseCode::Pattern { .. } = cause.code() {
            if let ty::Adt(expected_adt, substs) = exp_found.expected.kind() {
                let compatible_variants: Vec<_> = expected_adt
                    .variants()
                    .iter()
                    .filter(|variant| {
                        variant.fields.len() == 1 && variant.ctor_kind() == Some(CtorKind::Fn)
                    })
                    .filter_map(|variant| {
                        let sole_field = &variant.fields[0];
                        let sole_field_ty = sole_field.ty(self.tcx, substs);
                        if self.same_type_modulo_infer(sole_field_ty, exp_found.found) {
                            let variant_path =
                                with_no_trimmed_paths!(self.tcx.def_path_str(variant.def_id));
                            // FIXME #56861: DRYer prelude filtering
                            if let Some(path) = variant_path.strip_prefix("std::prelude::") {
                                if let Some((_, path)) = path.split_once("::") {
                                    return Some(path.to_string());
                                }
                            }
                            Some(variant_path)
                        } else {
                            None
                        }
                    })
                    .collect();
                match &compatible_variants[..] {
                    [] => {}
                    [variant] => {
                        diag.multipart_suggestion_verbose(
                            &format!("try wrapping the pattern in `{}`", variant),
                            vec![
                                (cause.span.shrink_to_lo(), format!("{}(", variant)),
                                (cause.span.shrink_to_hi(), ")".to_string()),
                            ],
                            Applicability::MaybeIncorrect,
                        );
                    }
                    _ => {
                        // More than one matching variant.
                        diag.multipart_suggestions(
                            &format!(
                                "try wrapping the pattern in a variant of `{}`",
                                self.tcx.def_path_str(expected_adt.did())
                            ),
                            compatible_variants.into_iter().map(|variant| {
                                vec![
                                    (cause.span.shrink_to_lo(), format!("{}(", variant)),
                                    (cause.span.shrink_to_hi(), ")".to_string()),
                                ]
                            }),
                            Applicability::MaybeIncorrect,
                        );
                    }
                }
            }
        }
    }

    /// A possible error is to forget to add `.await` when using futures:
    ///
    /// ```compile_fail,E0308
    /// async fn make_u32() -> u32 {
    ///     22
    /// }
    ///
    /// fn take_u32(x: u32) {}
    ///
    /// async fn foo() {
    ///     let x = make_u32();
    ///     take_u32(x);
    /// }
    /// ```
    ///
    /// This routine checks if the found type `T` implements `Future<Output=U>` where `U` is the
    /// expected type. If this is the case, and we are inside of an async body, it suggests adding
    /// `.await` to the tail of the expression.
    pub(super) fn suggest_await_on_expect_found(
        &self,
        cause: &ObligationCause<'tcx>,
        exp_span: Span,
        exp_found: &ty::error::ExpectedFound<Ty<'tcx>>,
        diag: &mut Diagnostic,
    ) {
        debug!(
            "suggest_await_on_expect_found: exp_span={:?}, expected_ty={:?}, found_ty={:?}",
            exp_span, exp_found.expected, exp_found.found,
        );

        if let ObligationCauseCode::CompareImplItemObligation { .. } = cause.code() {
            return;
        }

        match (
            self.get_impl_future_output_ty(exp_found.expected),
            self.get_impl_future_output_ty(exp_found.found),
        ) {
            (Some(exp), Some(found)) if self.same_type_modulo_infer(exp, found) => match cause
                .code()
            {
                ObligationCauseCode::IfExpression(box IfExpressionCause { then_id, .. }) => {
                    let then_span = self.find_block_span_from_hir_id(*then_id);
                    diag.multipart_suggestion(
                        "consider `await`ing on both `Future`s",
                        vec![
                            (then_span.shrink_to_hi(), ".await".to_string()),
                            (exp_span.shrink_to_hi(), ".await".to_string()),
                        ],
                        Applicability::MaybeIncorrect,
                    );
                }
                ObligationCauseCode::MatchExpressionArm(box MatchExpressionArmCause {
                    prior_arms,
                    ..
                }) => {
                    if let [.., arm_span] = &prior_arms[..] {
                        diag.multipart_suggestion(
                            "consider `await`ing on both `Future`s",
                            vec![
                                (arm_span.shrink_to_hi(), ".await".to_string()),
                                (exp_span.shrink_to_hi(), ".await".to_string()),
                            ],
                            Applicability::MaybeIncorrect,
                        );
                    } else {
                        diag.help("consider `await`ing on both `Future`s");
                    }
                }
                _ => {
                    diag.help("consider `await`ing on both `Future`s");
                }
            },
            (_, Some(ty)) if self.same_type_modulo_infer(exp_found.expected, ty) => {
                diag.span_suggestion_verbose(
                    exp_span.shrink_to_hi(),
                    "consider `await`ing on the `Future`",
                    ".await",
                    Applicability::MaybeIncorrect,
                );
            }
            (Some(ty), _) if self.same_type_modulo_infer(ty, exp_found.found) => match cause.code()
            {
                ObligationCauseCode::Pattern { span: Some(then_span), .. } => {
                    diag.span_suggestion_verbose(
                        then_span.shrink_to_hi(),
                        "consider `await`ing on the `Future`",
                        ".await",
                        Applicability::MaybeIncorrect,
                    );
                }
                ObligationCauseCode::IfExpression(box IfExpressionCause { then_id, .. }) => {
                    let then_span = self.find_block_span_from_hir_id(*then_id);
                    diag.span_suggestion_verbose(
                        then_span.shrink_to_hi(),
                        "consider `await`ing on the `Future`",
                        ".await",
                        Applicability::MaybeIncorrect,
                    );
                }
                ObligationCauseCode::MatchExpressionArm(box MatchExpressionArmCause {
                    ref prior_arms,
                    ..
                }) => {
                    diag.multipart_suggestion_verbose(
                        "consider `await`ing on the `Future`",
                        prior_arms
                            .iter()
                            .map(|arm| (arm.shrink_to_hi(), ".await".to_string()))
                            .collect(),
                        Applicability::MaybeIncorrect,
                    );
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub(super) fn suggest_accessing_field_where_appropriate(
        &self,
        cause: &ObligationCause<'tcx>,
        exp_found: &ty::error::ExpectedFound<Ty<'tcx>>,
        diag: &mut Diagnostic,
    ) {
        debug!(
            "suggest_accessing_field_where_appropriate(cause={:?}, exp_found={:?})",
            cause, exp_found
        );
        if let ty::Adt(expected_def, expected_substs) = exp_found.expected.kind() {
            if expected_def.is_enum() {
                return;
            }

            if let Some((name, ty)) = expected_def
                .non_enum_variant()
                .fields
                .iter()
                .filter(|field| field.vis.is_accessible_from(field.did, self.tcx))
                .map(|field| (field.name, field.ty(self.tcx, expected_substs)))
                .find(|(_, ty)| self.same_type_modulo_infer(*ty, exp_found.found))
            {
                if let ObligationCauseCode::Pattern { span: Some(span), .. } = *cause.code() {
                    if let Ok(snippet) = self.tcx.sess.source_map().span_to_snippet(span) {
                        let suggestion = if expected_def.is_struct() {
                            format!("{}.{}", snippet, name)
                        } else if expected_def.is_union() {
                            format!("unsafe {{ {}.{} }}", snippet, name)
                        } else {
                            return;
                        };
                        diag.span_suggestion(
                            span,
                            &format!(
                                "you might have meant to use field `{}` whose type is `{}`",
                                name, ty
                            ),
                            suggestion,
                            Applicability::MaybeIncorrect,
                        );
                    }
                }
            }
        }
    }

    /// When encountering a case where `.as_ref()` on a `Result` or `Option` would be appropriate,
    /// suggests it.
    pub(super) fn suggest_as_ref_where_appropriate(
        &self,
        span: Span,
        exp_found: &ty::error::ExpectedFound<Ty<'tcx>>,
        diag: &mut Diagnostic,
    ) {
        if let Ok(snippet) = self.tcx.sess.source_map().span_to_snippet(span)
            && let Some(msg) = self.should_suggest_as_ref(exp_found.expected, exp_found.found)
        {
            diag.span_suggestion(
                span,
                msg,
                // HACK: fix issue# 100605, suggesting convert from &Option<T> to Option<&T>, remove the extra `&`
                format!("{}.as_ref()", snippet.trim_start_matches('&')),
                Applicability::MachineApplicable,
            );
        }
    }

    pub fn should_suggest_as_ref(&self, expected: Ty<'tcx>, found: Ty<'tcx>) -> Option<&str> {
        if let (ty::Adt(exp_def, exp_substs), ty::Ref(_, found_ty, _)) =
            (expected.kind(), found.kind())
        {
            if let ty::Adt(found_def, found_substs) = *found_ty.kind() {
                if exp_def == &found_def {
                    let have_as_ref = &[
                        (
                            sym::Option,
                            "you can convert from `&Option<T>` to `Option<&T>` using \
                        `.as_ref()`",
                        ),
                        (
                            sym::Result,
                            "you can convert from `&Result<T, E>` to \
                        `Result<&T, &E>` using `.as_ref()`",
                        ),
                    ];
                    if let Some(msg) = have_as_ref.iter().find_map(|(name, msg)| {
                        self.tcx.is_diagnostic_item(*name, exp_def.did()).then_some(msg)
                    }) {
                        let mut show_suggestion = true;
                        for (exp_ty, found_ty) in
                            std::iter::zip(exp_substs.types(), found_substs.types())
                        {
                            match *exp_ty.kind() {
                                ty::Ref(_, exp_ty, _) => {
                                    match (exp_ty.kind(), found_ty.kind()) {
                                        (_, ty::Param(_))
                                        | (_, ty::Infer(_))
                                        | (ty::Param(_), _)
                                        | (ty::Infer(_), _) => {}
                                        _ if self.same_type_modulo_infer(exp_ty, found_ty) => {}
                                        _ => show_suggestion = false,
                                    };
                                }
                                ty::Param(_) | ty::Infer(_) => {}
                                _ => show_suggestion = false,
                            }
                        }
                        if show_suggestion {
                            return Some(*msg);
                        }
                    }
                }
            }
        }
        None
    }

    /// Try to find code with pattern `if Some(..) = expr`
    /// use a `visitor` to mark the `if` which its span contains given error span,
    /// and then try to find a assignment in the `cond` part, which span is equal with error span
    pub(super) fn suggest_let_for_letchains(
        &self,
        err: &mut Diagnostic,
        cause: &ObligationCause<'_>,
        span: Span,
    ) {
        let hir = self.tcx.hir();
        let fn_hir_id = hir.get_parent_node(cause.body_id);
        if let Some(node) = self.tcx.hir().find(fn_hir_id) &&
            let hir::Node::Item(hir::Item {
                    kind: hir::ItemKind::Fn(_sig, _, body_id), ..
                }) = node {
        let body = hir.body(*body_id);

        /// Find the if expression with given span
        struct IfVisitor {
            pub result: bool,
            pub found_if: bool,
            pub err_span: Span,
        }

        impl<'v> Visitor<'v> for IfVisitor {
            fn visit_expr(&mut self, ex: &'v hir::Expr<'v>) {
                if self.result { return; }
                match ex.kind {
                    hir::ExprKind::If(cond, _, _) => {
                        self.found_if = true;
                        walk_expr(self, cond);
                        self.found_if = false;
                    }
                    _ => walk_expr(self, ex),
                }
            }

            fn visit_stmt(&mut self, ex: &'v hir::Stmt<'v>) {
                if let hir::StmtKind::Local(hir::Local {
                        span, pat: hir::Pat{..}, ty: None, init: Some(_), ..
                    }) = &ex.kind
                    && self.found_if
                    && span.eq(&self.err_span) {
                        self.result = true;
                }
                walk_stmt(self, ex);
            }

            fn visit_body(&mut self, body: &'v hir::Body<'v>) {
                hir::intravisit::walk_body(self, body);
            }
        }

        let mut visitor = IfVisitor { err_span: span, found_if: false, result: false };
        visitor.visit_body(&body);
        if visitor.result {
                err.subdiagnostic(SuggAddLetForLetChains{span: span.shrink_to_lo()});
            }
        }
    }
}

impl<'tcx> TypeErrCtxt<'_, 'tcx> {
    /// Be helpful when the user wrote `{... expr; }` and taking the `;` off
    /// is enough to fix the error.
    pub fn could_remove_semicolon(
        &self,
        blk: &'tcx hir::Block<'tcx>,
        expected_ty: Ty<'tcx>,
    ) -> Option<(Span, StatementAsExpression)> {
        let blk = blk.innermost_block();
        // Do not suggest if we have a tail expr.
        if blk.expr.is_some() {
            return None;
        }
        let last_stmt = blk.stmts.last()?;
        let hir::StmtKind::Semi(ref last_expr) = last_stmt.kind else {
            return None;
        };
        let last_expr_ty = self.typeck_results.as_ref()?.expr_ty_opt(*last_expr)?;
        let needs_box = match (last_expr_ty.kind(), expected_ty.kind()) {
            _ if last_expr_ty.references_error() => return None,
            _ if self.same_type_modulo_infer(last_expr_ty, expected_ty) => {
                StatementAsExpression::CorrectType
            }
            (ty::Opaque(last_def_id, _), ty::Opaque(exp_def_id, _))
                if last_def_id == exp_def_id =>
            {
                StatementAsExpression::CorrectType
            }
            (ty::Opaque(last_def_id, last_bounds), ty::Opaque(exp_def_id, exp_bounds)) => {
                debug!(
                    "both opaque, likely future {:?} {:?} {:?} {:?}",
                    last_def_id, last_bounds, exp_def_id, exp_bounds
                );

                let last_local_id = last_def_id.as_local()?;
                let exp_local_id = exp_def_id.as_local()?;

                match (
                    &self.tcx.hir().expect_item(last_local_id).kind,
                    &self.tcx.hir().expect_item(exp_local_id).kind,
                ) {
                    (
                        hir::ItemKind::OpaqueTy(hir::OpaqueTy { bounds: last_bounds, .. }),
                        hir::ItemKind::OpaqueTy(hir::OpaqueTy { bounds: exp_bounds, .. }),
                    ) if std::iter::zip(*last_bounds, *exp_bounds).all(|(left, right)| {
                        match (left, right) {
                            (
                                hir::GenericBound::Trait(tl, ml),
                                hir::GenericBound::Trait(tr, mr),
                            ) if tl.trait_ref.trait_def_id() == tr.trait_ref.trait_def_id()
                                && ml == mr =>
                            {
                                true
                            }
                            (
                                hir::GenericBound::LangItemTrait(langl, _, _, argsl),
                                hir::GenericBound::LangItemTrait(langr, _, _, argsr),
                            ) if langl == langr => {
                                // FIXME: consider the bounds!
                                debug!("{:?} {:?}", argsl, argsr);
                                true
                            }
                            _ => false,
                        }
                    }) =>
                    {
                        StatementAsExpression::NeedsBoxing
                    }
                    _ => StatementAsExpression::CorrectType,
                }
            }
            _ => return None,
        };
        let span = if last_stmt.span.from_expansion() {
            let mac_call = rustc_span::source_map::original_sp(last_stmt.span, blk.span);
            self.tcx.sess.source_map().mac_call_stmt_semi_span(mac_call)?
        } else {
            last_stmt.span.with_lo(last_stmt.span.hi() - BytePos(1))
        };
        Some((span, needs_box))
    }

    /// Suggest returning a local binding with a compatible type if the block
    /// has no return expression.
    pub fn consider_returning_binding(
        &self,
        blk: &'tcx hir::Block<'tcx>,
        expected_ty: Ty<'tcx>,
        err: &mut Diagnostic,
    ) -> bool {
        let blk = blk.innermost_block();
        // Do not suggest if we have a tail expr.
        if blk.expr.is_some() {
            return false;
        }
        let mut shadowed = FxIndexSet::default();
        let mut candidate_idents = vec![];
        let mut find_compatible_candidates = |pat: &hir::Pat<'_>| {
            if let hir::PatKind::Binding(_, hir_id, ident, _) = &pat.kind
                && let Some(pat_ty) = self
                    .typeck_results
                    .as_ref()
                    .and_then(|typeck_results| typeck_results.node_type_opt(*hir_id))
            {
                let pat_ty = self.resolve_vars_if_possible(pat_ty);
                if self.same_type_modulo_infer(pat_ty, expected_ty)
                    && !(pat_ty, expected_ty).references_error()
                    && shadowed.insert(ident.name)
                {
                    candidate_idents.push((*ident, pat_ty));
                }
            }
            true
        };

        let hir = self.tcx.hir();
        for stmt in blk.stmts.iter().rev() {
            let hir::StmtKind::Local(local) = &stmt.kind else { continue; };
            local.pat.walk(&mut find_compatible_candidates);
        }
        match hir.find(hir.get_parent_node(blk.hir_id)) {
            Some(hir::Node::Expr(hir::Expr { hir_id, .. })) => {
                match hir.find(hir.get_parent_node(*hir_id)) {
                    Some(hir::Node::Arm(hir::Arm { pat, .. })) => {
                        pat.walk(&mut find_compatible_candidates);
                    }
                    Some(
                        hir::Node::Item(hir::Item { kind: hir::ItemKind::Fn(_, _, body), .. })
                        | hir::Node::ImplItem(hir::ImplItem {
                            kind: hir::ImplItemKind::Fn(_, body),
                            ..
                        })
                        | hir::Node::TraitItem(hir::TraitItem {
                            kind: hir::TraitItemKind::Fn(_, hir::TraitFn::Provided(body)),
                            ..
                        })
                        | hir::Node::Expr(hir::Expr {
                            kind: hir::ExprKind::Closure(hir::Closure { body, .. }),
                            ..
                        }),
                    ) => {
                        for param in hir.body(*body).params {
                            param.pat.walk(&mut find_compatible_candidates);
                        }
                    }
                    Some(hir::Node::Expr(hir::Expr {
                        kind:
                            hir::ExprKind::If(
                                hir::Expr { kind: hir::ExprKind::Let(let_), .. },
                                then_block,
                                _,
                            ),
                        ..
                    })) if then_block.hir_id == *hir_id => {
                        let_.pat.walk(&mut find_compatible_candidates);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        match &candidate_idents[..] {
            [(ident, _ty)] => {
                let sm = self.tcx.sess.source_map();
                if let Some(stmt) = blk.stmts.last() {
                    let stmt_span = sm.stmt_span(stmt.span, blk.span);
                    let sugg = if sm.is_multiline(blk.span)
                        && let Some(spacing) = sm.indentation_before(stmt_span)
                    {
                        format!("\n{spacing}{ident}")
                    } else {
                        format!(" {ident}")
                    };
                    err.span_suggestion_verbose(
                        stmt_span.shrink_to_hi(),
                        format!("consider returning the local binding `{ident}`"),
                        sugg,
                        Applicability::MaybeIncorrect,
                    );
                } else {
                    let sugg = if sm.is_multiline(blk.span)
                        && let Some(spacing) = sm.indentation_before(blk.span.shrink_to_lo())
                    {
                        format!("\n{spacing}    {ident}\n{spacing}")
                    } else {
                        format!(" {ident} ")
                    };
                    let left_span = sm.span_through_char(blk.span, '{').shrink_to_hi();
                    err.span_suggestion_verbose(
                        sm.span_extend_while(left_span, |c| c.is_whitespace()).unwrap_or(left_span),
                        format!("consider returning the local binding `{ident}`"),
                        sugg,
                        Applicability::MaybeIncorrect,
                    );
                }
                true
            }
            values if (1..3).contains(&values.len()) => {
                let spans = values.iter().map(|(ident, _)| ident.span).collect::<Vec<_>>();
                err.span_note(spans, "consider returning one of these bindings");
                true
            }
            _ => false,
        }
    }
}
