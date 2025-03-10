session_binary_float_literal_not_supported = binary float literal is not supported
session_branch_protection_requires_aarch64 = `-Zbranch-protection` is only supported on aarch64

session_cannot_enable_crt_static_linux = sanitizer is incompatible with statically linked libc, disable it using `-C target-feature=-crt-static`

session_cannot_mix_and_match_sanitizers = `-Zsanitizer={$first}` is incompatible with `-Zsanitizer={$second}`

session_cgu_not_recorded =
    CGU-reuse for `{$cgu_user_name}` is (mangled: `{$cgu_name}`) was not recorded

session_crate_name_does_not_match = `--crate-name` and `#[crate_name]` are required to match, but `{$s}` != `{$name}`

session_crate_name_empty = crate name must not be empty

session_crate_name_invalid = crate names cannot start with a `-`, but `{$s}` has a leading hyphen

session_expr_parentheses_needed = parentheses are required to parse this as an expression

session_feature_diagnostic_for_issue =
    see issue #{$n} <https://github.com/rust-lang/rust/issues/{$n}> for more information

session_feature_diagnostic_help =
    add `#![feature({$feature})]` to the crate attributes to enable

session_feature_gate_error = {$explain}

session_file_is_not_writeable = output file {$file} is not writeable -- check its permissions

session_file_write_fail = failed to write `{$path}` due to error `{$err}`

session_hexadecimal_float_literal_not_supported = hexadecimal float literal is not supported

session_incompatible_linker_flavor = linker flavor `{$flavor}` is incompatible with the current target
    .note = compatible flavors are: {$compatible_list}

session_incorrect_cgu_reuse_type =
    CGU-reuse for `{$cgu_user_name}` is `{$actual_reuse}` but should be {$at_least ->
    [one] {"at least "}
    *[other] {""}
    }`{$expected_reuse}`

session_instrumentation_not_supported = {$us} instrumentation is not supported for this target

session_int_literal_too_large = integer literal is too large
    .note = value exceeds limit of `{$limit}`

session_invalid_character_in_create_name = invalid character `{$character}` in crate name: `{$crate_name}`
session_invalid_character_in_create_name_help = you can either pass `--crate-name` on the command line or add `#![crate_name="…"]` to set the crate name

session_invalid_float_literal_suffix = invalid suffix `{$suffix}` for float literal
    .label = invalid suffix `{$suffix}`
    .help = valid suffixes are `f32` and `f64`

session_invalid_float_literal_width = invalid width `{$width}` for float literal
    .help = valid widths are 32 and 64

session_invalid_int_literal_width = invalid width `{$width}` for integer literal
    .help = valid widths are 8, 16, 32, 64 and 128

session_invalid_literal_suffix = suffixes on {$kind} literals are invalid
    .label = invalid suffix `{$suffix}`

session_invalid_num_literal_base_prefix = invalid base prefix for number literal
    .note = base prefixes (`0xff`, `0b1010`, `0o755`) are lowercase
    .suggestion = try making the prefix lowercase

session_invalid_num_literal_suffix = invalid suffix `{$suffix}` for number literal
    .label = invalid suffix `{$suffix}`
    .help = the suffix must be one of the numeric types (`u32`, `isize`, `f32`, etc.)

session_linker_plugin_lto_windows_not_supported = linker plugin based LTO is not supported together with `-C prefer-dynamic` when targeting Windows-like targets

session_not_circumvent_feature = `-Zunleash-the-miri-inside-of-you` may not be used to circumvent feature gates, except when testing error paths in the CTFE engine

session_not_supported = not supported

session_nul_in_c_str = null characters in C string literals are not supported

session_octal_float_literal_not_supported = octal float literal is not supported
session_optimization_fuel_exhausted = optimization-fuel-exhausted: {$msg}

session_profile_sample_use_file_does_not_exist = file `{$path}` passed to `-C profile-sample-use` does not exist.

session_profile_use_file_does_not_exist = file `{$path}` passed to `-C profile-use` does not exist.

session_sanitizer_cfi_canonical_jump_tables_requires_cfi = `-Zsanitizer-cfi-canonical-jump-tables` requires `-Zsanitizer=cfi`

session_sanitizer_cfi_generalize_pointers_requires_cfi = `-Zsanitizer-cfi-generalize-pointers` requires `-Zsanitizer=cfi` or `-Zsanitizer=kcfi`

session_sanitizer_cfi_normalize_integers_requires_cfi = `-Zsanitizer-cfi-normalize-integers` requires `-Zsanitizer=cfi` or `-Zsanitizer=kcfi`

session_sanitizer_cfi_requires_lto = `-Zsanitizer=cfi` requires `-Clto` or `-Clinker-plugin-lto`

session_sanitizer_cfi_requires_single_codegen_unit = `-Zsanitizer=cfi` with `-Clto` requires `-Ccodegen-units=1`

session_sanitizer_not_supported = {$us} sanitizer is not supported for this target

session_sanitizers_not_supported = {$us} sanitizers are not supported for this target

session_skipping_const_checks = skipping const checks
session_split_debuginfo_unstable_platform = `-Csplit-debuginfo={$debuginfo}` is unstable on this platform

session_split_lto_unit_requires_lto = `-Zsplit-lto-unit` requires `-Clto`, `-Clto=thin`, or `-Clinker-plugin-lto`

session_target_requires_unwind_tables = target requires unwind tables, they cannot be disabled with `-C force-unwind-tables=no`

session_target_stack_protector_not_supported = `-Z stack-protector={$stack_protector}` is not supported for target {$target_triple} and will be ignored

session_unleashed_feature_help_named = skipping check for `{$gate}` feature
session_unleashed_feature_help_unnamed = skipping check that does not even have a feature gate

session_unstable_virtual_function_elimination = `-Zvirtual-function-elimination` requires `-Clto`

session_unsupported_dwarf_version = requested DWARF version {$dwarf_version} is greater than 5

session_field_literal_too_large = field literal is too large
    .note = value exceeds limit of `{$limit}`
