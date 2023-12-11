//! Module that implements what will become the rustc side of Stable MIR.
//!
//! This module is responsible for building Stable MIR components from internal components.
//!
//! This module is not intended to be invoked directly by users. It will eventually
//! become the public API of rustc that will be invoked by the `stable_mir` crate.
//!
//! For now, we are developing everything inside `rustc`, thus, we keep this module private.

use crate::rustc_internal::{self, opaque};
use crate::stable_mir::mir::{CopyNonOverlapping, UserTypeProjection, VariantIdx};
use crate::stable_mir::ty::{
    allocation_filter, new_allocation, Const, FloatTy, IntTy, Movability, RigidTy, TyKind, UintTy,
};
use crate::stable_mir::ty::{CurveTy, FieldTy};
use crate::stable_mir::{self, Context};
use rustc_hir as hir;
use rustc_middle::mir::coverage::CodeRegion;
use rustc_middle::mir::interpret::alloc_range;
use rustc_middle::mir::{self, ConstantKind};
use rustc_middle::ty::{self, Ty, TyCtxt, Variance};
use rustc_span::def_id::{CrateNum, DefId, LOCAL_CRATE};
use rustc_target::abi::FieldIdx;
use tracing::debug;

impl<'tcx> Context for Tables<'tcx> {
    fn local_crate(&self) -> stable_mir::Crate {
        smir_crate(self.tcx, LOCAL_CRATE)
    }

    fn external_crates(&self) -> Vec<stable_mir::Crate> {
        self.tcx.crates(()).iter().map(|crate_num| smir_crate(self.tcx, *crate_num)).collect()
    }

    fn find_crate(&self, name: &str) -> Option<stable_mir::Crate> {
        [LOCAL_CRATE].iter().chain(self.tcx.crates(()).iter()).find_map(|crate_num| {
            let crate_name = self.tcx.crate_name(*crate_num).to_string();
            (name == crate_name).then(|| smir_crate(self.tcx, *crate_num))
        })
    }

    fn all_local_items(&mut self) -> stable_mir::CrateItems {
        self.tcx.mir_keys(()).iter().map(|item| self.crate_item(item.to_def_id())).collect()
    }
    fn entry_fn(&mut self) -> Option<stable_mir::CrateItem> {
        Some(self.crate_item(self.tcx.entry_fn(())?.0))
    }

    fn all_trait_decls(&mut self) -> stable_mir::TraitDecls {
        self.tcx
            .traits(LOCAL_CRATE)
            .iter()
            .map(|trait_def_id| self.trait_def(*trait_def_id))
            .collect()
    }

    fn trait_decl(&mut self, trait_def: &stable_mir::ty::TraitDef) -> stable_mir::ty::TraitDecl {
        let def_id = self.trait_def_id(trait_def);
        let trait_def = self.tcx.trait_def(def_id);
        trait_def.stable(self)
    }

    fn all_trait_impls(&mut self) -> stable_mir::ImplTraitDecls {
        self.tcx
            .trait_impls_in_crate(LOCAL_CRATE)
            .iter()
            .map(|impl_def_id| self.impl_def(*impl_def_id))
            .collect()
    }

    fn trait_impl(&mut self, impl_def: &stable_mir::ty::ImplDef) -> stable_mir::ty::ImplTrait {
        let def_id = self.impl_trait_def_id(impl_def);
        let impl_trait = self.tcx.impl_trait_ref(def_id).unwrap();
        impl_trait.stable(self)
    }

    fn mir_body(&mut self, item: &stable_mir::CrateItem) -> stable_mir::mir::Body {
        let def_id = self.item_def_id(item);
        let mir = self.tcx.optimized_mir(def_id);
        stable_mir::mir::Body {
            blocks: mir
                .basic_blocks
                .iter()
                .map(|block| stable_mir::mir::BasicBlock {
                    terminator: block.terminator().stable(self),
                    statements: block
                        .statements
                        .iter()
                        .map(|statement| statement.stable(self))
                        .collect(),
                })
                .collect(),
            locals: mir.local_decls.iter().map(|decl| self.intern_ty(decl.ty)).collect(),
        }
    }

    fn rustc_tables(&mut self, f: &mut dyn FnMut(&mut Tables<'_>)) {
        f(self)
    }

    fn ty_kind(&mut self, ty: crate::stable_mir::ty::Ty) -> TyKind {
        let ty = self.types[ty.0];
        ty.stable(self)
    }
}

pub struct Tables<'tcx> {
    pub tcx: TyCtxt<'tcx>,
    pub def_ids: Vec<DefId>,
    pub types: Vec<Ty<'tcx>>,
}

impl<'tcx> Tables<'tcx> {
    fn intern_ty(&mut self, ty: Ty<'tcx>) -> stable_mir::ty::Ty {
        if let Some(id) = self.types.iter().position(|&t| t == ty) {
            return stable_mir::ty::Ty(id);
        }
        let id = self.types.len();
        self.types.push(ty);
        stable_mir::ty::Ty(id)
    }
}

/// Build a stable mir crate from a given crate number.
fn smir_crate(tcx: TyCtxt<'_>, crate_num: CrateNum) -> stable_mir::Crate {
    let crate_name = tcx.crate_name(crate_num).to_string();
    let is_local = crate_num == LOCAL_CRATE;
    debug!(?crate_name, ?crate_num, "smir_crate");
    stable_mir::Crate { id: crate_num.into(), name: crate_name, is_local }
}

/// Trait used to convert between an internal MIR type to a Stable MIR type.
pub(crate) trait Stable<'tcx> {
    /// The stable representation of the type implementing Stable.
    type T;
    /// Converts an object to the equivalent Stable MIR representation.
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T;
}

impl<'tcx> Stable<'tcx> for mir::Statement<'tcx> {
    type T = stable_mir::mir::Statement;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::StatementKind::*;
        match &self.kind {
            Assign(assign) => {
                stable_mir::mir::Statement::Assign(assign.0.stable(tables), assign.1.stable(tables))
            }
            FakeRead(fake_read_place) => stable_mir::mir::Statement::FakeRead(
                fake_read_place.0.stable(tables),
                fake_read_place.1.stable(tables),
            ),
            SetDiscriminant { place: plc, variant_index: idx } => {
                stable_mir::mir::Statement::SetDiscriminant {
                    place: plc.as_ref().stable(tables),
                    variant_index: idx.stable(tables),
                }
            }
            Deinit(place) => stable_mir::mir::Statement::Deinit(place.stable(tables)),
            StorageLive(place) => stable_mir::mir::Statement::StorageLive(place.stable(tables)),
            StorageDead(place) => stable_mir::mir::Statement::StorageDead(place.stable(tables)),
            Retag(retag, place) => {
                stable_mir::mir::Statement::Retag(retag.stable(tables), place.stable(tables))
            }
            PlaceMention(place) => stable_mir::mir::Statement::PlaceMention(place.stable(tables)),
            AscribeUserType(place_projection, variance) => {
                stable_mir::mir::Statement::AscribeUserType {
                    place: place_projection.as_ref().0.stable(tables),
                    projections: place_projection.as_ref().1.stable(tables),
                    variance: variance.stable(tables),
                }
            }
            Coverage(coverage) => stable_mir::mir::Statement::Coverage(stable_mir::mir::Coverage {
                kind: coverage.kind.stable(tables),
                code_region: coverage.code_region.as_ref().map(|reg| reg.stable(tables)),
            }),
            Intrinsic(intrinstic) => {
                stable_mir::mir::Statement::Intrinsic(intrinstic.stable(tables))
            }
            ConstEvalCounter => stable_mir::mir::Statement::ConstEvalCounter,
            Nop => stable_mir::mir::Statement::Nop,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::Rvalue<'tcx> {
    type T = stable_mir::mir::Rvalue;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use mir::Rvalue::*;
        match self {
            Use(op) => stable_mir::mir::Rvalue::Use(op.stable(tables)),
            Repeat(op, len) => {
                let cnst = ConstantKind::from_const(*len, tables.tcx);
                let len = Const { literal: cnst.stable(tables) };
                stable_mir::mir::Rvalue::Repeat(op.stable(tables), len)
            }
            Ref(region, kind, place) => stable_mir::mir::Rvalue::Ref(
                opaque(region),
                kind.stable(tables),
                place.stable(tables),
            ),
            ThreadLocalRef(def_id) => {
                stable_mir::mir::Rvalue::ThreadLocalRef(rustc_internal::crate_item(*def_id))
            }
            AddressOf(mutability, place) => {
                stable_mir::mir::Rvalue::AddressOf(mutability.stable(tables), place.stable(tables))
            }
            Len(place) => stable_mir::mir::Rvalue::Len(place.stable(tables)),
            Cast(cast_kind, op, ty) => stable_mir::mir::Rvalue::Cast(
                cast_kind.stable(tables),
                op.stable(tables),
                tables.intern_ty(*ty),
            ),
            BinaryOp(bin_op, ops) => stable_mir::mir::Rvalue::BinaryOp(
                bin_op.stable(tables),
                ops.0.stable(tables),
                ops.1.stable(tables),
            ),
            CheckedBinaryOp(bin_op, ops) => stable_mir::mir::Rvalue::CheckedBinaryOp(
                bin_op.stable(tables),
                ops.0.stable(tables),
                ops.1.stable(tables),
            ),
            NullaryOp(null_op, ty) => {
                stable_mir::mir::Rvalue::NullaryOp(null_op.stable(tables), tables.intern_ty(*ty))
            }
            UnaryOp(un_op, op) => {
                stable_mir::mir::Rvalue::UnaryOp(un_op.stable(tables), op.stable(tables))
            }
            Discriminant(place) => stable_mir::mir::Rvalue::Discriminant(place.stable(tables)),
            Aggregate(agg_kind, operands) => {
                let operands = operands.iter().map(|op| op.stable(tables)).collect();
                stable_mir::mir::Rvalue::Aggregate(agg_kind.stable(tables), operands)
            }
            ShallowInitBox(op, ty) => {
                stable_mir::mir::Rvalue::ShallowInitBox(op.stable(tables), tables.intern_ty(*ty))
            }
            CopyForDeref(place) => stable_mir::mir::Rvalue::CopyForDeref(place.stable(tables)),
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::Mutability {
    type T = stable_mir::mir::Mutability;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use mir::Mutability::*;
        match *self {
            Not => stable_mir::mir::Mutability::Not,
            Mut => stable_mir::mir::Mutability::Mut,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::BorrowKind {
    type T = stable_mir::mir::BorrowKind;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use mir::BorrowKind::*;
        match *self {
            Shared => stable_mir::mir::BorrowKind::Shared,
            Shallow => stable_mir::mir::BorrowKind::Shallow,
            Mut { kind } => stable_mir::mir::BorrowKind::Mut { kind: kind.stable(tables) },
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::MutBorrowKind {
    type T = stable_mir::mir::MutBorrowKind;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use mir::MutBorrowKind::*;
        match *self {
            Default => stable_mir::mir::MutBorrowKind::Default,
            TwoPhaseBorrow => stable_mir::mir::MutBorrowKind::TwoPhaseBorrow,
            ClosureCapture => stable_mir::mir::MutBorrowKind::ClosureCapture,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::NullOp<'tcx> {
    type T = stable_mir::mir::NullOp;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use mir::NullOp::*;
        match self {
            SizeOf => stable_mir::mir::NullOp::SizeOf,
            AlignOf => stable_mir::mir::NullOp::AlignOf,
            OffsetOf(indices) => stable_mir::mir::NullOp::OffsetOf(
                indices.iter().map(|idx| idx.stable(tables)).collect(),
            ),
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::CastKind {
    type T = stable_mir::mir::CastKind;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use mir::CastKind::*;
        match self {
            PointerExposeAddress => stable_mir::mir::CastKind::PointerExposeAddress,
            PointerFromExposedAddress => stable_mir::mir::CastKind::PointerFromExposedAddress,
            PointerCoercion(c) => stable_mir::mir::CastKind::PointerCoercion(c.stable(tables)),
            DynStar => stable_mir::mir::CastKind::DynStar,
            IntToInt => stable_mir::mir::CastKind::IntToInt,
            FloatToInt => stable_mir::mir::CastKind::FloatToInt,
            FloatToFloat => stable_mir::mir::CastKind::FloatToFloat,
            IntToFloat => stable_mir::mir::CastKind::IntToFloat,
            PtrToPtr => stable_mir::mir::CastKind::PtrToPtr,
            FnPtrToPtr => stable_mir::mir::CastKind::FnPtrToPtr,
            Transmute => stable_mir::mir::CastKind::Transmute,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::AliasKind {
    type T = stable_mir::ty::AliasKind;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use ty::AliasKind::*;
        match self {
            Projection => stable_mir::ty::AliasKind::Projection,
            Inherent => stable_mir::ty::AliasKind::Inherent,
            Opaque => stable_mir::ty::AliasKind::Opaque,
            Weak => stable_mir::ty::AliasKind::Weak,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::AliasTy<'tcx> {
    type T = stable_mir::ty::AliasTy;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        let ty::AliasTy { args, def_id, .. } = self;
        stable_mir::ty::AliasTy { def_id: tables.alias_def(*def_id), args: args.stable(tables) }
    }
}

impl<'tcx> Stable<'tcx> for ty::DynKind {
    type T = stable_mir::ty::DynKind;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use ty::DynKind;
        match self {
            DynKind::Dyn => stable_mir::ty::DynKind::Dyn,
            DynKind::DynStar => stable_mir::ty::DynKind::DynStar,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::ExistentialPredicate<'tcx> {
    type T = stable_mir::ty::ExistentialPredicate;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::ExistentialPredicate::*;
        match self {
            ty::ExistentialPredicate::Trait(existential_trait_ref) => {
                Trait(existential_trait_ref.stable(tables))
            }
            ty::ExistentialPredicate::Projection(existential_projection) => {
                Projection(existential_projection.stable(tables))
            }
            ty::ExistentialPredicate::AutoTrait(def_id) => AutoTrait(tables.trait_def(*def_id)),
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::ExistentialTraitRef<'tcx> {
    type T = stable_mir::ty::ExistentialTraitRef;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        let ty::ExistentialTraitRef { def_id, args } = self;
        stable_mir::ty::ExistentialTraitRef {
            def_id: tables.trait_def(*def_id),
            generic_args: args.stable(tables),
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::TermKind<'tcx> {
    type T = stable_mir::ty::TermKind;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::TermKind;
        match self {
            ty::TermKind::Ty(ty) => TermKind::Type(tables.intern_ty(*ty)),
            ty::TermKind::Const(cnst) => {
                let cnst = ConstantKind::from_const(*cnst, tables.tcx);
                let cnst = Const { literal: cnst.stable(tables) };
                TermKind::Const(cnst)
            }
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::ExistentialProjection<'tcx> {
    type T = stable_mir::ty::ExistentialProjection;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        let ty::ExistentialProjection { def_id, args, term } = self;
        stable_mir::ty::ExistentialProjection {
            def_id: tables.trait_def(*def_id),
            generic_args: args.stable(tables),
            term: term.unpack().stable(tables),
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::adjustment::PointerCoercion {
    type T = stable_mir::mir::PointerCoercion;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use ty::adjustment::PointerCoercion;
        match self {
            PointerCoercion::ReifyFnPointer => stable_mir::mir::PointerCoercion::ReifyFnPointer,
            PointerCoercion::UnsafeFnPointer => stable_mir::mir::PointerCoercion::UnsafeFnPointer,
            PointerCoercion::ClosureFnPointer(unsafety) => {
                stable_mir::mir::PointerCoercion::ClosureFnPointer(unsafety.stable(tables))
            }
            PointerCoercion::MutToConstPointer => {
                stable_mir::mir::PointerCoercion::MutToConstPointer
            }
            PointerCoercion::ArrayToPointer => stable_mir::mir::PointerCoercion::ArrayToPointer,
            PointerCoercion::Unsize => stable_mir::mir::PointerCoercion::Unsize,
        }
    }
}

impl<'tcx> Stable<'tcx> for rustc_hir::Unsafety {
    type T = stable_mir::mir::Safety;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            rustc_hir::Unsafety::Unsafe => stable_mir::mir::Safety::Unsafe,
            rustc_hir::Unsafety::Normal => stable_mir::mir::Safety::Normal,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::FakeReadCause {
    type T = stable_mir::mir::FakeReadCause;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use mir::FakeReadCause::*;
        match self {
            ForMatchGuard => stable_mir::mir::FakeReadCause::ForMatchGuard,
            ForMatchedPlace(local_def_id) => {
                stable_mir::mir::FakeReadCause::ForMatchedPlace(opaque(local_def_id))
            }
            ForGuardBinding => stable_mir::mir::FakeReadCause::ForGuardBinding,
            ForLet(local_def_id) => stable_mir::mir::FakeReadCause::ForLet(opaque(local_def_id)),
            ForIndex => stable_mir::mir::FakeReadCause::ForIndex,
        }
    }
}

impl<'tcx> Stable<'tcx> for FieldIdx {
    type T = usize;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        self.as_usize()
    }
}

impl<'tcx> Stable<'tcx> for mir::Operand<'tcx> {
    type T = stable_mir::mir::Operand;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use mir::Operand::*;
        match self {
            Copy(place) => stable_mir::mir::Operand::Copy(place.stable(tables)),
            Move(place) => stable_mir::mir::Operand::Move(place.stable(tables)),
            Constant(c) => stable_mir::mir::Operand::Constant(c.to_string()),
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::Place<'tcx> {
    type T = stable_mir::mir::Place;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        stable_mir::mir::Place {
            local: self.local.as_usize(),
            projection: format!("{:?}", self.projection),
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::coverage::CoverageKind {
    type T = stable_mir::mir::CoverageKind;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::coverage::CoverageKind;
        match self {
            CoverageKind::Counter { function_source_hash, id } => {
                stable_mir::mir::CoverageKind::Counter {
                    function_source_hash: *function_source_hash as usize,
                    id: opaque(id),
                }
            }
            CoverageKind::Expression { id, lhs, op, rhs } => {
                stable_mir::mir::CoverageKind::Expression {
                    id: opaque(id),
                    lhs: opaque(lhs),
                    op: op.stable(tables),
                    rhs: opaque(rhs),
                }
            }
            CoverageKind::Unreachable => stable_mir::mir::CoverageKind::Unreachable,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::UserTypeProjection {
    type T = stable_mir::mir::UserTypeProjection;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        UserTypeProjection { base: self.base.as_usize(), projection: format!("{:?}", self.projs) }
    }
}

impl<'tcx> Stable<'tcx> for mir::coverage::Op {
    type T = stable_mir::mir::Op;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::coverage::Op::*;
        match self {
            Subtract => stable_mir::mir::Op::Subtract,
            Add => stable_mir::mir::Op::Add,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::Local {
    type T = stable_mir::mir::Local;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        self.as_usize()
    }
}

impl<'tcx> Stable<'tcx> for rustc_target::abi::VariantIdx {
    type T = VariantIdx;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        self.as_usize()
    }
}

impl<'tcx> Stable<'tcx> for Variance {
    type T = stable_mir::mir::Variance;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            Variance::Bivariant => stable_mir::mir::Variance::Bivariant,
            Variance::Contravariant => stable_mir::mir::Variance::Contravariant,
            Variance::Covariant => stable_mir::mir::Variance::Covariant,
            Variance::Invariant => stable_mir::mir::Variance::Invariant,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::RetagKind {
    type T = stable_mir::mir::RetagKind;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::RetagKind;
        match self {
            RetagKind::FnEntry => stable_mir::mir::RetagKind::FnEntry,
            RetagKind::TwoPhase => stable_mir::mir::RetagKind::TwoPhase,
            RetagKind::Raw => stable_mir::mir::RetagKind::Raw,
            RetagKind::Default => stable_mir::mir::RetagKind::Default,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::UserTypeAnnotationIndex {
    type T = usize;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        self.as_usize()
    }
}

impl<'tcx> Stable<'tcx> for CodeRegion {
    type T = stable_mir::mir::CodeRegion;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        stable_mir::mir::CodeRegion {
            file_name: self.file_name.as_str().to_string(),
            start_line: self.start_line as usize,
            start_col: self.start_col as usize,
            end_line: self.end_line as usize,
            end_col: self.end_col as usize,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::UnwindAction {
    type T = stable_mir::mir::UnwindAction;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::UnwindAction;
        match self {
            UnwindAction::Continue => stable_mir::mir::UnwindAction::Continue,
            UnwindAction::Unreachable => stable_mir::mir::UnwindAction::Unreachable,
            UnwindAction::Terminate => stable_mir::mir::UnwindAction::Terminate,
            UnwindAction::Cleanup(bb) => stable_mir::mir::UnwindAction::Cleanup(bb.as_usize()),
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::NonDivergingIntrinsic<'tcx> {
    type T = stable_mir::mir::NonDivergingIntrinsic;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::NonDivergingIntrinsic;
        match self {
            NonDivergingIntrinsic::Assume(op) => {
                stable_mir::mir::NonDivergingIntrinsic::Assume(op.stable(tables))
            }
            NonDivergingIntrinsic::CopyNonOverlapping(copy_non_overlapping) => {
                stable_mir::mir::NonDivergingIntrinsic::CopyNonOverlapping(CopyNonOverlapping {
                    src: copy_non_overlapping.src.stable(tables),
                    dst: copy_non_overlapping.dst.stable(tables),
                    count: copy_non_overlapping.count.stable(tables),
                })
            }
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::AssertMessage<'tcx> {
    type T = stable_mir::mir::AssertMessage;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::AssertKind;
        match self {
            AssertKind::BoundsCheck { len, index } => stable_mir::mir::AssertMessage::BoundsCheck {
                len: len.stable(tables),
                index: index.stable(tables),
            },
            AssertKind::Overflow(bin_op, op1, op2) => stable_mir::mir::AssertMessage::Overflow(
                bin_op.stable(tables),
                op1.stable(tables),
                op2.stable(tables),
            ),
            AssertKind::OverflowNeg(op) => {
                stable_mir::mir::AssertMessage::OverflowNeg(op.stable(tables))
            }
            AssertKind::DivisionByZero(op) => {
                stable_mir::mir::AssertMessage::DivisionByZero(op.stable(tables))
            }
            AssertKind::RemainderByZero(op) => {
                stable_mir::mir::AssertMessage::RemainderByZero(op.stable(tables))
            }
            AssertKind::ResumedAfterReturn(generator) => {
                stable_mir::mir::AssertMessage::ResumedAfterReturn(generator.stable(tables))
            }
            AssertKind::ResumedAfterPanic(generator) => {
                stable_mir::mir::AssertMessage::ResumedAfterPanic(generator.stable(tables))
            }
            AssertKind::MisalignedPointerDereference { required, found } => {
                stable_mir::mir::AssertMessage::MisalignedPointerDereference {
                    required: required.stable(tables),
                    found: found.stable(tables),
                }
            }
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::BinOp {
    type T = stable_mir::mir::BinOp;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use mir::BinOp;
        match self {
            BinOp::Add => stable_mir::mir::BinOp::Add,
            BinOp::AddUnchecked => stable_mir::mir::BinOp::AddUnchecked,
            BinOp::Sub => stable_mir::mir::BinOp::Sub,
            BinOp::SubUnchecked => stable_mir::mir::BinOp::SubUnchecked,
            BinOp::Mul => stable_mir::mir::BinOp::Mul,
            BinOp::MulUnchecked => stable_mir::mir::BinOp::MulUnchecked,
            BinOp::Div => stable_mir::mir::BinOp::Div,
            BinOp::Rem => stable_mir::mir::BinOp::Rem,
            BinOp::BitXor => stable_mir::mir::BinOp::BitXor,
            BinOp::BitAnd => stable_mir::mir::BinOp::BitAnd,
            BinOp::BitOr => stable_mir::mir::BinOp::BitOr,
            BinOp::Shl => stable_mir::mir::BinOp::Shl,
            BinOp::ShlUnchecked => stable_mir::mir::BinOp::ShlUnchecked,
            BinOp::Shr => stable_mir::mir::BinOp::Shr,
            BinOp::ShrUnchecked => stable_mir::mir::BinOp::ShrUnchecked,
            BinOp::Eq => stable_mir::mir::BinOp::Eq,
            BinOp::Lt => stable_mir::mir::BinOp::Lt,
            BinOp::Le => stable_mir::mir::BinOp::Le,
            BinOp::Ne => stable_mir::mir::BinOp::Ne,
            BinOp::Ge => stable_mir::mir::BinOp::Ge,
            BinOp::Gt => stable_mir::mir::BinOp::Gt,
            BinOp::Offset => stable_mir::mir::BinOp::Offset,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::UnOp {
    type T = stable_mir::mir::UnOp;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use mir::UnOp;
        match self {
            UnOp::Not => stable_mir::mir::UnOp::Not,
            UnOp::Neg => stable_mir::mir::UnOp::Neg,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::AggregateKind<'tcx> {
    type T = stable_mir::mir::AggregateKind;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        match self {
            mir::AggregateKind::Array(ty) => {
                stable_mir::mir::AggregateKind::Array(tables.intern_ty(*ty))
            }
            mir::AggregateKind::Tuple => stable_mir::mir::AggregateKind::Tuple,
            mir::AggregateKind::Adt(def_id, var_idx, generic_arg, user_ty_index, field_idx) => {
                stable_mir::mir::AggregateKind::Adt(
                    rustc_internal::adt_def(*def_id),
                    var_idx.index(),
                    generic_arg.stable(tables),
                    user_ty_index.map(|idx| idx.index()),
                    field_idx.map(|idx| idx.index()),
                )
            }
            mir::AggregateKind::Closure(def_id, generic_arg) => {
                stable_mir::mir::AggregateKind::Closure(
                    rustc_internal::closure_def(*def_id),
                    generic_arg.stable(tables),
                )
            }
            mir::AggregateKind::Generator(def_id, generic_arg, movability) => {
                stable_mir::mir::AggregateKind::Generator(
                    rustc_internal::generator_def(*def_id),
                    generic_arg.stable(tables),
                    movability.stable(tables),
                )
            }
        }
    }
}

impl<'tcx> Stable<'tcx> for rustc_hir::GeneratorKind {
    type T = stable_mir::mir::GeneratorKind;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use rustc_hir::{AsyncGeneratorKind, GeneratorKind};
        match self {
            GeneratorKind::Async(async_gen) => {
                let async_gen = match async_gen {
                    AsyncGeneratorKind::Block => stable_mir::mir::AsyncGeneratorKind::Block,
                    AsyncGeneratorKind::Closure => stable_mir::mir::AsyncGeneratorKind::Closure,
                    AsyncGeneratorKind::Fn => stable_mir::mir::AsyncGeneratorKind::Fn,
                };
                stable_mir::mir::GeneratorKind::Async(async_gen)
            }
            GeneratorKind::Gen => stable_mir::mir::GeneratorKind::Gen,
        }
    }
}

impl<'tcx> Stable<'tcx> for mir::InlineAsmOperand<'tcx> {
    type T = stable_mir::mir::InlineAsmOperand;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::InlineAsmOperand;

        let (in_value, out_place) = match self {
            InlineAsmOperand::In { value, .. } => (Some(value.stable(tables)), None),
            InlineAsmOperand::Out { place, .. } => (None, place.map(|place| place.stable(tables))),
            InlineAsmOperand::InOut { in_value, out_place, .. } => {
                (Some(in_value.stable(tables)), out_place.map(|place| place.stable(tables)))
            }
            InlineAsmOperand::Const { .. }
            | InlineAsmOperand::SymFn { .. }
            | InlineAsmOperand::SymStatic { .. } => (None, None),
        };

        stable_mir::mir::InlineAsmOperand { in_value, out_place, raw_rpr: format!("{self:?}") }
    }
}

impl<'tcx> Stable<'tcx> for mir::Terminator<'tcx> {
    type T = stable_mir::mir::Terminator;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use rustc_middle::mir::TerminatorKind::*;
        use stable_mir::mir::Terminator;
        match &self.kind {
            Goto { target } => Terminator::Goto { target: target.as_usize() },
            SwitchInt { discr, targets } => Terminator::SwitchInt {
                discr: discr.stable(tables),
                targets: targets
                    .iter()
                    .map(|(value, target)| stable_mir::mir::SwitchTarget {
                        value,
                        target: target.as_usize(),
                    })
                    .collect(),
                otherwise: targets.otherwise().as_usize(),
            },
            Resume => Terminator::Resume,
            Terminate => Terminator::Abort,
            Return => Terminator::Return,
            Unreachable => Terminator::Unreachable,
            Drop { place, target, unwind, replace: _ } => Terminator::Drop {
                place: place.stable(tables),
                target: target.as_usize(),
                unwind: unwind.stable(tables),
            },
            Call { func, args, destination, target, unwind, call_source: _, fn_span: _ } => {
                Terminator::Call {
                    func: func.stable(tables),
                    args: args.iter().map(|arg| arg.stable(tables)).collect(),
                    destination: destination.stable(tables),
                    target: target.map(|t| t.as_usize()),
                    unwind: unwind.stable(tables),
                }
            }
            Assert { cond, expected, msg, target, unwind } => Terminator::Assert {
                cond: cond.stable(tables),
                expected: *expected,
                msg: msg.stable(tables),
                target: target.as_usize(),
                unwind: unwind.stable(tables),
            },
            InlineAsm { template, operands, options, line_spans, destination, unwind } => {
                Terminator::InlineAsm {
                    template: format!("{template:?}"),
                    operands: operands.iter().map(|operand| operand.stable(tables)).collect(),
                    options: format!("{options:?}"),
                    line_spans: format!("{line_spans:?}"),
                    destination: destination.map(|d| d.as_usize()),
                    unwind: unwind.stable(tables),
                }
            }
            Yield { .. } | GeneratorDrop | FalseEdge { .. } | FalseUnwind { .. } => unreachable!(),
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::GenericArgs<'tcx> {
    type T = stable_mir::ty::GenericArgs;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::GenericArgs;

        GenericArgs(self.iter().map(|arg| arg.unpack().stable(tables)).collect())
    }
}

impl<'tcx> Stable<'tcx> for ty::GenericArgKind<'tcx> {
    type T = stable_mir::ty::GenericArgKind;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::GenericArgKind;
        match self {
            ty::GenericArgKind::Lifetime(region) => GenericArgKind::Lifetime(opaque(region)),
            ty::GenericArgKind::Type(ty) => GenericArgKind::Type(tables.intern_ty(*ty)),
            ty::GenericArgKind::Const(cnst) => {
                let cnst = ConstantKind::from_const(*cnst, tables.tcx);
                GenericArgKind::Const(stable_mir::ty::Const { literal: cnst.stable(tables) })
            }
        }
    }
}

impl<'tcx, S, V> Stable<'tcx> for ty::Binder<'tcx, S>
where
    S: Stable<'tcx, T = V>,
{
    type T = stable_mir::ty::Binder<V>;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::Binder;

        Binder {
            value: self.as_ref().skip_binder().stable(tables),
            bound_vars: self
                .bound_vars()
                .iter()
                .map(|bound_var| bound_var.stable(tables))
                .collect(),
        }
    }
}

impl<'tcx, S, V> Stable<'tcx> for ty::EarlyBinder<S>
where
    S: Stable<'tcx, T = V>,
{
    type T = stable_mir::ty::EarlyBinder<V>;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::EarlyBinder;

        EarlyBinder { value: self.as_ref().skip_binder().stable(tables) }
    }
}

impl<'tcx> Stable<'tcx> for ty::FnSig<'tcx> {
    type T = stable_mir::ty::FnSig;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use rustc_target::spec::abi;
        use stable_mir::ty::{Abi, FnSig};

        FnSig {
            inputs_and_output: self
                .inputs_and_output
                .iter()
                .map(|ty| tables.intern_ty(ty))
                .collect(),
            c_variadic: self.c_variadic,
            unsafety: self.unsafety.stable(tables),
            abi: match self.abi {
                abi::Abi::Rust => Abi::Rust,
                abi::Abi::C { unwind } => Abi::C { unwind },
                abi::Abi::Cdecl { unwind } => Abi::Cdecl { unwind },
                abi::Abi::Stdcall { unwind } => Abi::Stdcall { unwind },
                abi::Abi::Fastcall { unwind } => Abi::Fastcall { unwind },
                abi::Abi::Vectorcall { unwind } => Abi::Vectorcall { unwind },
                abi::Abi::Thiscall { unwind } => Abi::Thiscall { unwind },
                abi::Abi::Aapcs { unwind } => Abi::Aapcs { unwind },
                abi::Abi::Win64 { unwind } => Abi::Win64 { unwind },
                abi::Abi::SysV64 { unwind } => Abi::SysV64 { unwind },
                abi::Abi::PtxKernel => Abi::PtxKernel,
                abi::Abi::Msp430Interrupt => Abi::Msp430Interrupt,
                abi::Abi::X86Interrupt => Abi::X86Interrupt,
                abi::Abi::AmdGpuKernel => Abi::AmdGpuKernel,
                abi::Abi::EfiApi => Abi::EfiApi,
                abi::Abi::AvrInterrupt => Abi::AvrInterrupt,
                abi::Abi::AvrNonBlockingInterrupt => Abi::AvrNonBlockingInterrupt,
                abi::Abi::CCmseNonSecureCall => Abi::CCmseNonSecureCall,
                abi::Abi::Wasm => Abi::Wasm,
                abi::Abi::System { unwind } => Abi::System { unwind },
                abi::Abi::RustIntrinsic => Abi::RustIntrinsic,
                abi::Abi::RustCall => Abi::RustCall,
                abi::Abi::PlatformIntrinsic => Abi::PlatformIntrinsic,
                abi::Abi::Unadjusted => Abi::Unadjusted,
                abi::Abi::RustCold => Abi::RustCold,
                abi::Abi::RiscvInterruptM => Abi::RiscvInterruptM,
                abi::Abi::RiscvInterruptS => Abi::RiscvInterruptS,
            },
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::BoundTyKind {
    type T = stable_mir::ty::BoundTyKind;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::BoundTyKind;

        match self {
            ty::BoundTyKind::Anon => BoundTyKind::Anon,
            ty::BoundTyKind::Param(def_id, symbol) => {
                BoundTyKind::Param(rustc_internal::param_def(*def_id), symbol.to_string())
            }
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::BoundRegionKind {
    type T = stable_mir::ty::BoundRegionKind;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::BoundRegionKind;

        match self {
            ty::BoundRegionKind::BrAnon(option_span) => {
                BoundRegionKind::BrAnon(option_span.map(|span| opaque(&span)))
            }
            ty::BoundRegionKind::BrNamed(def_id, symbol) => {
                BoundRegionKind::BrNamed(rustc_internal::br_named_def(*def_id), symbol.to_string())
            }
            ty::BoundRegionKind::BrEnv => BoundRegionKind::BrEnv,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::BoundVariableKind {
    type T = stable_mir::ty::BoundVariableKind;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::BoundVariableKind;

        match self {
            ty::BoundVariableKind::Ty(bound_ty_kind) => {
                BoundVariableKind::Ty(bound_ty_kind.stable(tables))
            }
            ty::BoundVariableKind::Region(bound_region_kind) => {
                BoundVariableKind::Region(bound_region_kind.stable(tables))
            }
            ty::BoundVariableKind::Const => BoundVariableKind::Const,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::IntTy {
    type T = IntTy;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            ty::IntTy::Isize => IntTy::Isize,
            ty::IntTy::I8 => IntTy::I8,
            ty::IntTy::I16 => IntTy::I16,
            ty::IntTy::I32 => IntTy::I32,
            ty::IntTy::I64 => IntTy::I64,
            ty::IntTy::I128 => IntTy::I128,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::UintTy {
    type T = UintTy;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            ty::UintTy::Usize => UintTy::Usize,
            ty::UintTy::U8 => UintTy::U8,
            ty::UintTy::U16 => UintTy::U16,
            ty::UintTy::U32 => UintTy::U32,
            ty::UintTy::U64 => UintTy::U64,
            ty::UintTy::U128 => UintTy::U128,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::FloatTy {
    type T = FloatTy;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            ty::FloatTy::F32 => FloatTy::F32,
            ty::FloatTy::F64 => FloatTy::F64,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::FieldTy {
    type T = FieldTy;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            ty::FieldTy::Bls12381Base => FieldTy::Bls12381Base,
            ty::FieldTy::Bls12381Scalar => FieldTy::Bls12381Scalar,
            ty::FieldTy::Curve25519Base => FieldTy::Curve25519Base,
            ty::FieldTy::Curve25519Scalar => FieldTy::Curve25519Scalar,
            ty::FieldTy::PallasBase => FieldTy::PallasBase,
            ty::FieldTy::PallasScalar => FieldTy::PallasScalar,
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::CurveTy {
    type T = CurveTy;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            ty::CurveTy::Bls12381 => CurveTy::Bls12381,
            ty::CurveTy::Curve25519 => CurveTy::Curve25519,
            ty::CurveTy::Pallas => CurveTy::Pallas,
            ty::CurveTy::Vesta => CurveTy::Vesta,
        }
    }
}

impl<'tcx> Stable<'tcx> for hir::Movability {
    type T = Movability;

    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            hir::Movability::Static => Movability::Static,
            hir::Movability::Movable => Movability::Movable,
        }
    }
}

impl<'tcx> Stable<'tcx> for Ty<'tcx> {
    type T = stable_mir::ty::TyKind;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        match self.kind() {
            ty::Bool => TyKind::RigidTy(RigidTy::Bool),
            ty::Char => TyKind::RigidTy(RigidTy::Char),
            ty::Int(int_ty) => TyKind::RigidTy(RigidTy::Int(int_ty.stable(tables))),
            ty::Uint(uint_ty) => TyKind::RigidTy(RigidTy::Uint(uint_ty.stable(tables))),
            ty::Float(float_ty) => TyKind::RigidTy(RigidTy::Float(float_ty.stable(tables))),
            ty::Field(field_ty) => TyKind::RigidTy(RigidTy::Field(field_ty.stable(tables))),
            ty::Curve(curve_ty) => TyKind::RigidTy(RigidTy::Curve(curve_ty.stable(tables))),
            ty::Adt(adt_def, generic_args) => TyKind::RigidTy(RigidTy::Adt(
                rustc_internal::adt_def(adt_def.did()),
                generic_args.stable(tables),
            )),
            ty::Foreign(def_id) => {
                TyKind::RigidTy(RigidTy::Foreign(rustc_internal::foreign_def(*def_id)))
            }
            ty::Str => TyKind::RigidTy(RigidTy::Str),
            ty::Array(ty, constant) => {
                let cnst = ConstantKind::from_const(*constant, tables.tcx);
                let cnst = stable_mir::ty::Const { literal: cnst.stable(tables) };
                TyKind::RigidTy(RigidTy::Array(tables.intern_ty(*ty), cnst))
            }
            ty::Slice(ty) => TyKind::RigidTy(RigidTy::Slice(tables.intern_ty(*ty))),
            ty::RawPtr(ty::TypeAndMut { ty, mutbl }) => {
                TyKind::RigidTy(RigidTy::RawPtr(tables.intern_ty(*ty), mutbl.stable(tables)))
            }
            ty::Ref(region, ty, mutbl) => TyKind::RigidTy(RigidTy::Ref(
                opaque(region),
                tables.intern_ty(*ty),
                mutbl.stable(tables),
            )),
            ty::FnDef(def_id, generic_args) => TyKind::RigidTy(RigidTy::FnDef(
                rustc_internal::fn_def(*def_id),
                generic_args.stable(tables),
            )),
            ty::FnPtr(poly_fn_sig) => TyKind::RigidTy(RigidTy::FnPtr(poly_fn_sig.stable(tables))),
            ty::Dynamic(existential_predicates, region, dyn_kind) => {
                TyKind::RigidTy(RigidTy::Dynamic(
                    existential_predicates
                        .iter()
                        .map(|existential_predicate| existential_predicate.stable(tables))
                        .collect(),
                    opaque(region),
                    dyn_kind.stable(tables),
                ))
            }
            ty::Closure(def_id, generic_args) => TyKind::RigidTy(RigidTy::Closure(
                rustc_internal::closure_def(*def_id),
                generic_args.stable(tables),
            )),
            ty::Generator(def_id, generic_args, movability) => TyKind::RigidTy(RigidTy::Generator(
                rustc_internal::generator_def(*def_id),
                generic_args.stable(tables),
                movability.stable(tables),
            )),
            ty::Never => TyKind::RigidTy(RigidTy::Never),
            ty::Tuple(fields) => TyKind::RigidTy(RigidTy::Tuple(
                fields.iter().map(|ty| tables.intern_ty(ty)).collect(),
            )),
            ty::Alias(alias_kind, alias_ty) => {
                TyKind::Alias(alias_kind.stable(tables), alias_ty.stable(tables))
            }
            ty::Param(param_ty) => TyKind::Param(param_ty.stable(tables)),
            ty::Bound(debruijn_idx, bound_ty) => {
                TyKind::Bound(debruijn_idx.as_usize(), bound_ty.stable(tables))
            }
            ty::Placeholder(..)
            | ty::GeneratorWitness(_)
            | ty::GeneratorWitnessMIR(_, _)
            | ty::Infer(_)
            | ty::Error(_) => {
                unreachable!();
            }
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::ParamTy {
    type T = stable_mir::ty::ParamTy;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::ParamTy;
        ParamTy { index: self.index, name: self.name.to_string() }
    }
}

impl<'tcx> Stable<'tcx> for ty::BoundTy {
    type T = stable_mir::ty::BoundTy;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::BoundTy;
        BoundTy { var: self.var.as_usize(), kind: self.kind.stable(tables) }
    }
}

impl<'tcx> Stable<'tcx> for mir::interpret::Allocation {
    type T = stable_mir::ty::Allocation;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        allocation_filter(self, alloc_range(rustc_target::abi::Size::ZERO, self.size()), tables)
    }
}

impl<'tcx> Stable<'tcx> for ty::trait_def::TraitSpecializationKind {
    type T = stable_mir::ty::TraitSpecializationKind;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::TraitSpecializationKind;

        match self {
            ty::trait_def::TraitSpecializationKind::None => TraitSpecializationKind::None,
            ty::trait_def::TraitSpecializationKind::Marker => TraitSpecializationKind::Marker,
            ty::trait_def::TraitSpecializationKind::AlwaysApplicable => {
                TraitSpecializationKind::AlwaysApplicable
            }
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::TraitDef {
    type T = stable_mir::ty::TraitDecl;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::TraitDecl;

        TraitDecl {
            def_id: rustc_internal::trait_def(self.def_id),
            unsafety: self.unsafety.stable(tables),
            paren_sugar: self.paren_sugar,
            has_auto_impl: self.has_auto_impl,
            is_marker: self.is_marker,
            is_coinductive: self.is_coinductive,
            skip_array_during_method_dispatch: self.skip_array_during_method_dispatch,
            specialization_kind: self.specialization_kind.stable(tables),
            must_implement_one_of: self
                .must_implement_one_of
                .as_ref()
                .map(|idents| idents.iter().map(|ident| opaque(ident)).collect()),
            implement_via_object: self.implement_via_object,
            deny_explicit_impl: self.deny_explicit_impl,
        }
    }
}

impl<'tcx> Stable<'tcx> for rustc_middle::mir::ConstantKind<'tcx> {
    type T = stable_mir::ty::ConstantKind;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        match self {
            ConstantKind::Ty(c) => match c.kind() {
                ty::Value(val) => {
                    let const_val = tables.tcx.valtree_to_const_val((c.ty(), val));
                    stable_mir::ty::ConstantKind::Allocated(new_allocation(self, const_val, tables))
                }
                ty::ParamCt(param) => stable_mir::ty::ConstantKind::ParamCt(opaque(&param)),
                ty::ErrorCt(_) => unreachable!(),
                _ => unimplemented!(),
            },
            ConstantKind::Unevaluated(unev_const, ty) => {
                stable_mir::ty::ConstantKind::Unevaluated(stable_mir::ty::UnevaluatedConst {
                    ty: tables.intern_ty(*ty),
                    def: tables.const_def(unev_const.def),
                    args: unev_const.args.stable(tables),
                    promoted: unev_const.promoted.map(|u| u.as_u32()),
                })
            }
            ConstantKind::Val(val, _) => {
                stable_mir::ty::ConstantKind::Allocated(new_allocation(self, *val, tables))
            }
        }
    }
}

impl<'tcx> Stable<'tcx> for ty::TraitRef<'tcx> {
    type T = stable_mir::ty::TraitRef;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use stable_mir::ty::TraitRef;

        TraitRef { def_id: rustc_internal::trait_def(self.def_id), args: self.args.stable(tables) }
    }
}
