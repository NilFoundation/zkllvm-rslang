use super::eval_queries::{mk_eval_cx, op_to_const};
use super::machine::CompileTimeEvalContext;
use super::{ValTreeCreationError, ValTreeCreationResult, VALTREE_MAX_NODES};
use crate::const_eval::CanAccessStatics;
use crate::interpret::MPlaceTy;
use crate::interpret::{
    intern_const_alloc_recursive, ConstValue, ImmTy, Immediate, InternKind, MemPlaceMeta,
    MemoryKind, Place, Projectable, Scalar,
};
use rustc_middle::ty::layout::{LayoutOf, TyAndLayout};
use rustc_middle::ty::{self, ScalarInt, Ty, TyCtxt};
use rustc_span::source_map::DUMMY_SP;
use rustc_target::abi::VariantIdx;

#[instrument(skip(ecx), level = "debug")]
fn branches<'tcx>(
    ecx: &CompileTimeEvalContext<'tcx, 'tcx>,
    place: &MPlaceTy<'tcx>,
    n: usize,
    variant: Option<VariantIdx>,
    num_nodes: &mut usize,
) -> ValTreeCreationResult<'tcx> {
    let place = match variant {
        Some(variant) => ecx.project_downcast(place, variant).unwrap(),
        None => place.clone(),
    };
    let variant = variant.map(|variant| Some(ty::ValTree::Leaf(ScalarInt::from(variant.as_u32()))));
    debug!(?place, ?variant);

    let mut fields = Vec::with_capacity(n);
    for i in 0..n {
        let field = ecx.project_field(&place, i).unwrap();
        let valtree = const_to_valtree_inner(ecx, &field, num_nodes)?;
        fields.push(Some(valtree));
    }

    // For enums, we prepend their variant index before the variant's fields so we can figure out
    // the variant again when just seeing a valtree.
    let branches = variant
        .into_iter()
        .chain(fields.into_iter())
        .collect::<Option<Vec<_>>>()
        .expect("should have already checked for errors in ValTree creation");

    // Have to account for ZSTs here
    if branches.len() == 0 {
        *num_nodes += 1;
    }

    Ok(ty::ValTree::Branch(ecx.tcx.arena.alloc_from_iter(branches)))
}

#[instrument(skip(ecx), level = "debug")]
fn slice_branches<'tcx>(
    ecx: &CompileTimeEvalContext<'tcx, 'tcx>,
    place: &MPlaceTy<'tcx>,
    num_nodes: &mut usize,
) -> ValTreeCreationResult<'tcx> {
    let n = place.len(ecx).unwrap_or_else(|_| panic!("expected to use len of place {place:?}"));

    let mut elems = Vec::with_capacity(n as usize);
    for i in 0..n {
        let place_elem = ecx.project_index(place, i).unwrap();
        let valtree = const_to_valtree_inner(ecx, &place_elem, num_nodes)?;
        elems.push(valtree);
    }

    Ok(ty::ValTree::Branch(ecx.tcx.arena.alloc_from_iter(elems)))
}

#[instrument(skip(ecx), level = "debug")]
pub(crate) fn const_to_valtree_inner<'tcx>(
    ecx: &CompileTimeEvalContext<'tcx, 'tcx>,
    place: &MPlaceTy<'tcx>,
    num_nodes: &mut usize,
) -> ValTreeCreationResult<'tcx> {
    let ty = place.layout.ty;
    debug!("ty kind: {:?}", ty.kind());

    if *num_nodes >= VALTREE_MAX_NODES {
        return Err(ValTreeCreationError::NodesOverflow);
    }

    match ty.kind() {
        ty::FnDef(..) => {
            *num_nodes += 1;
            Ok(ty::ValTree::zst())
        }
        ty::Bool | ty::Int(_) | ty::Uint(_) | ty::Field(_) | ty::Float(_) | ty::Char => {
            // FIXME(aleasims): should we really handle fields here like this?
            let Ok(val) = ecx.read_immediate(place) else {
                return Err(ValTreeCreationError::Other);
            };
            let val = val.to_scalar();
            *num_nodes += 1;

            Ok(ty::ValTree::Leaf(val.assert_int()))
        }

        // Raw pointers are not allowed in type level constants, as we cannot properly test them for
        // equality at compile-time (see `ptr_guaranteed_cmp`).
        // Technically we could allow function pointers (represented as `ty::Instance`), but this is not guaranteed to
        // agree with runtime equality tests.
        ty::FnPtr(_) | ty::RawPtr(_) => Err(ValTreeCreationError::NonSupportedType),

        ty::Ref(_, _, _)  => {
            let Ok(derefd_place)= ecx.deref_pointer(place) else {
                return Err(ValTreeCreationError::Other);
            };
            debug!(?derefd_place);

            const_to_valtree_inner(ecx, &derefd_place, num_nodes)
        }

        ty::Str | ty::Slice(_) | ty::Array(_, _) => {
            slice_branches(ecx, place, num_nodes)
        }
        // Trait objects are not allowed in type level constants, as we have no concept for
        // resolving their backing type, even if we can do that at const eval time. We may
        // hypothetically be able to allow `dyn StructuralEq` trait objects in the future,
        // but it is unclear if this is useful.
        ty::Dynamic(..) => Err(ValTreeCreationError::NonSupportedType),

        ty::Tuple(elem_tys) => {
            branches(ecx, place, elem_tys.len(), None, num_nodes)
        }

        ty::Adt(def, _) => {
            if def.is_union() {
                return Err(ValTreeCreationError::NonSupportedType);
            } else if def.variants().is_empty() {
                bug!("uninhabited types should have errored and never gotten converted to valtree")
            }

            let Ok(variant) = ecx.read_discriminant(place) else {
                return Err(ValTreeCreationError::Other);
            };
            branches(ecx, place, def.variant(variant).fields.len(), def.is_enum().then_some(variant), num_nodes)
        }

        ty::Never
        | ty::Error(_)
        | ty::Foreign(..)
        | ty::Curve(_)
        | ty::Infer(ty::FreshIntTy(_))
        | ty::Infer(ty::FreshFloatTy(_))
        // FIXME(oli-obk): we could look behind opaque types
        | ty::Alias(..)
        | ty::Param(_)
        | ty::Bound(..)
        | ty::Placeholder(..)
        | ty::Infer(_)
        // FIXME(oli-obk): we can probably encode closures just like structs
        | ty::Closure(..)
        | ty::Generator(..)
        | ty::GeneratorWitness(..) |ty::GeneratorWitnessMIR(..)=> Err(ValTreeCreationError::NonSupportedType),
    }
}

/// Valtrees don't store the `MemPlaceMeta` that all dynamically sized values have in the interpreter.
/// This function reconstructs it.
fn reconstruct_place_meta<'tcx>(
    layout: TyAndLayout<'tcx>,
    valtree: ty::ValTree<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> MemPlaceMeta {
    if layout.is_sized() {
        return MemPlaceMeta::None;
    }

    let mut last_valtree = valtree;
    // Traverse the type, and update `last_valtree` as we go.
    let tail = tcx.struct_tail_with_normalize(
        layout.ty,
        |ty| ty,
        || {
            let branches = last_valtree.unwrap_branch();
            last_valtree = *branches.last().unwrap();
            debug!(?branches, ?last_valtree);
        },
    );
    // Sanity-check that we got a tail we support.
    match tail.kind() {
        ty::Slice(..) | ty::Str => {}
        _ => bug!("unsized tail of a valtree must be Slice or Str"),
    };

    // Get the number of elements in the unsized field.
    let num_elems = last_valtree.unwrap_branch().len();
    MemPlaceMeta::Meta(Scalar::from_target_usize(num_elems as u64, &tcx))
}

#[instrument(skip(ecx), level = "debug", ret)]
fn create_pointee_place<'tcx>(
    ecx: &mut CompileTimeEvalContext<'tcx, 'tcx>,
    ty: Ty<'tcx>,
    valtree: ty::ValTree<'tcx>,
) -> MPlaceTy<'tcx> {
    let layout = ecx.layout_of(ty).unwrap();
    let meta = reconstruct_place_meta(layout, valtree, ecx.tcx.tcx);
    ecx.allocate_dyn(layout, MemoryKind::Stack, meta).unwrap()
}

/// Converts a `ValTree` to a `ConstValue`, which is needed after mir
/// construction has finished.
// FIXME Merge `valtree_to_const_value` and `valtree_into_mplace` into one function
#[instrument(skip(tcx), level = "debug", ret)]
pub fn valtree_to_const_value<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env_ty: ty::ParamEnvAnd<'tcx, Ty<'tcx>>,
    valtree: ty::ValTree<'tcx>,
) -> ConstValue<'tcx> {
    // Basic idea: We directly construct `Scalar` values from trivial `ValTree`s
    // (those for constants with type bool, int, uint, float or char).
    // For all other types we create an `MPlace` and fill that by walking
    // the `ValTree` and using `place_projection` and `place_field` to
    // create inner `MPlace`s which are filled recursively.
    // FIXME Does this need an example?

    let (param_env, ty) = param_env_ty.into_parts();
    let mut ecx: crate::interpret::InterpCx<
        '_,
        '_,
        crate::const_eval::CompileTimeInterpreter<'_, '_>,
    > = mk_eval_cx(tcx, DUMMY_SP, param_env, CanAccessStatics::No);

    match ty.kind() {
        ty::FnDef(..) => {
            assert!(valtree.unwrap_branch().is_empty());
            ConstValue::ZeroSized
        }
        ty::Bool | ty::Int(_) | ty::Uint(_) | ty::Field(_) | ty::Float(_) | ty::Char => match valtree {
            // FIXME(aleasims): fields shouldn't be here.
            ty::ValTree::Leaf(scalar_int) => ConstValue::Scalar(Scalar::Int(scalar_int)),
            ty::ValTree::Branch(_) => bug!(
                "ValTrees for Bool, Int, Uint, Float or Char should have the form ValTree::Leaf"
            ),
        },
        ty::Ref(_, _, _) | ty::Tuple(_) | ty::Array(_, _) | ty::Adt(..) => {
            let place = match ty.kind() {
                ty::Ref(_, inner_ty, _) => {
                    // Need to create a place for the pointee (the reference itself will be an immediate)
                    create_pointee_place(&mut ecx, *inner_ty, valtree)
                }
                _ => {
                    // Need to create a place for this valtree.
                    create_pointee_place(&mut ecx, ty, valtree)
                }
            };
            debug!(?place);

            valtree_into_mplace(&mut ecx, &place, valtree);
            dump_place(&ecx, &place);
            intern_const_alloc_recursive(&mut ecx, InternKind::Constant, &place).unwrap();

            match ty.kind() {
                ty::Ref(_, _, _) => {
                    let ref_place = place.to_ref(&tcx);
                    let imm =
                        ImmTy::from_immediate(ref_place, tcx.layout_of(param_env_ty).unwrap());

                    op_to_const(&ecx, &imm.into())
                }
                _ => op_to_const(&ecx, &place.into()),
            }
        }
        ty::Never
        | ty::Error(_)
        | ty::Foreign(..)
        | ty::Curve(_)
        | ty::Infer(ty::FreshIntTy(_))
        | ty::Infer(ty::FreshFloatTy(_))
        | ty::Alias(..)
        | ty::Param(_)
        | ty::Bound(..)
        | ty::Placeholder(..)
        | ty::Infer(_)
        | ty::Closure(..)
        | ty::Generator(..)
        | ty::GeneratorWitness(..)
        | ty::GeneratorWitnessMIR(..)
        | ty::FnPtr(_)
        | ty::RawPtr(_)
        | ty::Str
        | ty::Slice(_)
        | ty::Dynamic(..) => bug!("no ValTree should have been created for type {:?}", ty.kind()),
    }
}

#[instrument(skip(ecx), level = "debug")]
fn valtree_into_mplace<'tcx>(
    ecx: &mut CompileTimeEvalContext<'tcx, 'tcx>,
    place: &MPlaceTy<'tcx>,
    valtree: ty::ValTree<'tcx>,
) {
    // This will match on valtree and write the value(s) corresponding to the ValTree
    // inside the place recursively.

    let tcx = ecx.tcx.tcx;
    let ty = place.layout.ty;

    match ty.kind() {
        ty::FnDef(_, _) => {
            // Zero-sized type, nothing to do.
        }
        ty::Bool | ty::Int(_) | ty::Uint(_) | ty::Float(_) | ty::Char => {
            let scalar_int = valtree.unwrap_leaf();
            debug!("writing trivial valtree {:?} to place {:?}", scalar_int, place);
            ecx.write_immediate(Immediate::Scalar(scalar_int.into()), place).unwrap();
        }
        ty::Ref(_, inner_ty, _) => {
            let pointee_place = create_pointee_place(ecx, *inner_ty, valtree);
            debug!(?pointee_place);

            valtree_into_mplace(ecx, &pointee_place, valtree);
            dump_place(ecx, &pointee_place);
            intern_const_alloc_recursive(ecx, InternKind::Constant, &pointee_place).unwrap();

            let imm = match inner_ty.kind() {
                ty::Slice(_) | ty::Str => {
                    let len = valtree.unwrap_branch().len();
                    let len_scalar = Scalar::from_target_usize(len as u64, &tcx);

                    Immediate::ScalarPair(
                        Scalar::from_maybe_pointer((*pointee_place).ptr, &tcx),
                        len_scalar,
                    )
                }
                _ => pointee_place.to_ref(&tcx),
            };
            debug!(?imm);

            ecx.write_immediate(imm, place).unwrap();
        }
        ty::Adt(_, _) | ty::Tuple(_) | ty::Array(_, _) | ty::Str | ty::Slice(_) => {
            let branches = valtree.unwrap_branch();

            // Need to downcast place for enums
            let (place_adjusted, branches, variant_idx) = match ty.kind() {
                ty::Adt(def, _) if def.is_enum() => {
                    // First element of valtree corresponds to variant
                    let scalar_int = branches[0].unwrap_leaf();
                    let variant_idx = VariantIdx::from_u32(scalar_int.try_to_u32().unwrap());
                    let variant = def.variant(variant_idx);
                    debug!(?variant);

                    (
                        ecx.project_downcast(place, variant_idx).unwrap(),
                        &branches[1..],
                        Some(variant_idx),
                    )
                }
                _ => (place.clone(), branches, None),
            };
            debug!(?place_adjusted, ?branches);

            // Create the places (by indexing into `place`) for the fields and fill
            // them recursively
            for (i, inner_valtree) in branches.iter().enumerate() {
                debug!(?i, ?inner_valtree);

                let place_inner = match ty.kind() {
                    ty::Str | ty::Slice(_) | ty::Array(..) => {
                        ecx.project_index(place, i as u64).unwrap()
                    }
                    _ => ecx.project_field(&place_adjusted, i).unwrap(),
                };

                debug!(?place_inner);
                valtree_into_mplace(ecx, &place_inner, *inner_valtree);
                dump_place(&ecx, &place_inner);
            }

            debug!("dump of place_adjusted:");
            dump_place(ecx, &place_adjusted);

            if let Some(variant_idx) = variant_idx {
                // don't forget filling the place with the discriminant of the enum
                ecx.write_discriminant(variant_idx, place).unwrap();
            }

            debug!("dump of place after writing discriminant:");
            dump_place(ecx, place);
        }
        // FIXME(aleasims): fields shouldn't be here.
        ty::Field(_) => unimplemented!("no field constants yet"),
        _ => bug!("shouldn't have created a ValTree for {:?}", ty),
    }
}

fn dump_place<'tcx>(ecx: &CompileTimeEvalContext<'tcx, 'tcx>, place: &MPlaceTy<'tcx>) {
    trace!("{:?}", ecx.dump_place(Place::Ptr(**place)));
}
