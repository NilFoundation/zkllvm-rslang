hir_typeck_fru_note = this expression may have been misinterpreted as a `..` range expression
hir_typeck_fru_expr = this expression does not end in a comma...
hir_typeck_fru_expr2 = ... so this is interpreted as a `..` range expression, instead of functional record update syntax
hir_typeck_fru_suggestion =
    to set the remaining fields{$expr ->
        [NONE]{""}
        *[other] {" "}from `{$expr}`
    }, separate the last named field with a comma

hir_typeck_field_multiply_specified_in_initializer =
    field `{$ident}` specified more than once
    .label = used more than once
    .previous_use_label = first use of `{$ident}`

hir_typeck_return_stmt_outside_of_fn_body =
    return statement outside of function body
    .encl_body_label = the return is part of this body...
    .encl_fn_label = ...not the enclosing function body

hir_typeck_yield_expr_outside_of_generator =
    yield expression outside of generator literal

hir_typeck_struct_expr_non_exhaustive =
    cannot create non-exhaustive {$what} using struct expression

hir_typeck_method_call_on_unknown_type =
    the type of this value must be known to call a method on a raw pointer on it

hir_typeck_functional_record_update_on_non_struct =
    functional record update syntax requires a struct

hir_typeck_address_of_temporary_taken = cannot take address of a temporary
    .label = temporary value

hir_typeck_add_return_type_add = try adding a return type

hir_typeck_add_return_type_missing_here = a return type might be missing here

hir_typeck_expected_default_return_type = expected `()` because of default return type

hir_typeck_expected_return_type = expected `{$expected}` because of return type

hir_typeck_missing_parentheses_in_range = can't call method `{$method_name}` on type `{$ty_str}`

hir_typeck_add_missing_parentheses_in_range = you must surround the range in parentheses to call its `{$func_name}` function

hir_typeck_op_trait_generic_params =
    `{$method_name}` must not have any generic parameters
