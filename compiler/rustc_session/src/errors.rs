use std::num::NonZeroU32;

use crate::cgu_reuse_tracker::CguReuse;
use crate::parse::ParseSess;
use rustc_ast::token;
use rustc_ast::util::literal::LitError;
use rustc_errors::MultiSpan;
use rustc_macros::Diagnostic;
use rustc_span::{Span, Symbol};
use rustc_target::spec::{SplitDebuginfo, StackProtector, TargetTriple};

#[derive(Diagnostic)]
#[diag(session_incorrect_cgu_reuse_type)]
pub struct IncorrectCguReuseType<'a> {
    #[primary_span]
    pub span: Span,
    pub cgu_user_name: &'a str,
    pub actual_reuse: CguReuse,
    pub expected_reuse: CguReuse,
    pub at_least: u8,
}

#[derive(Diagnostic)]
#[diag(session_cgu_not_recorded)]
pub struct CguNotRecorded<'a> {
    pub cgu_user_name: &'a str,
    pub cgu_name: &'a str,
}

#[derive(Diagnostic)]
#[diag(session_feature_gate_error, code = "E0658")]
pub struct FeatureGateError<'a> {
    #[primary_span]
    pub span: MultiSpan,
    pub explain: &'a str,
}

#[derive(Subdiagnostic)]
#[note(session_feature_diagnostic_for_issue)]
pub struct FeatureDiagnosticForIssue {
    pub n: NonZeroU32,
}

#[derive(Subdiagnostic)]
#[help(session_feature_diagnostic_help)]
pub struct FeatureDiagnosticHelp {
    pub feature: Symbol,
}

#[derive(Diagnostic)]
#[diag(session_not_circumvent_feature)]
pub struct NotCircumventFeature;

#[derive(Diagnostic)]
#[diag(session_linker_plugin_lto_windows_not_supported)]
pub struct LinkerPluginToWindowsNotSupported;

#[derive(Diagnostic)]
#[diag(session_profile_use_file_does_not_exist)]
pub struct ProfileUseFileDoesNotExist<'a> {
    pub path: &'a std::path::Path,
}

#[derive(Diagnostic)]
#[diag(session_profile_sample_use_file_does_not_exist)]
pub struct ProfileSampleUseFileDoesNotExist<'a> {
    pub path: &'a std::path::Path,
}

#[derive(Diagnostic)]
#[diag(session_target_requires_unwind_tables)]
pub struct TargetRequiresUnwindTables;

#[derive(Diagnostic)]
#[diag(session_sanitizer_not_supported)]
pub struct SanitizerNotSupported {
    pub us: String,
}

#[derive(Diagnostic)]
#[diag(session_sanitizers_not_supported)]
pub struct SanitizersNotSupported {
    pub us: String,
}

#[derive(Diagnostic)]
#[diag(session_cannot_mix_and_match_sanitizers)]
pub struct CannotMixAndMatchSanitizers {
    pub first: String,
    pub second: String,
}

#[derive(Diagnostic)]
#[diag(session_cannot_enable_crt_static_linux)]
pub struct CannotEnableCrtStaticLinux;

#[derive(Diagnostic)]
#[diag(session_sanitizer_cfi_enabled)]
pub struct SanitizerCfiEnabled;

#[derive(Diagnostic)]
#[diag(session_unstable_virtual_function_elimination)]
pub struct UnstableVirtualFunctionElimination;

#[derive(Diagnostic)]
#[diag(session_unsupported_dwarf_version)]
pub struct UnsupportedDwarfVersion {
    pub dwarf_version: u32,
}

#[derive(Diagnostic)]
#[diag(session_target_stack_protector_not_supported)]
pub struct StackProtectorNotSupportedForTarget<'a> {
    pub stack_protector: StackProtector,
    pub target_triple: &'a TargetTriple,
}

#[derive(Diagnostic)]
#[diag(session_split_debuginfo_unstable_platform)]
pub struct SplitDebugInfoUnstablePlatform {
    pub debuginfo: SplitDebuginfo,
}

#[derive(Diagnostic)]
#[diag(session_file_is_not_writeable)]
pub struct FileIsNotWriteable<'a> {
    pub file: &'a std::path::Path,
}

#[derive(Diagnostic)]
#[diag(session_crate_name_does_not_match)]
pub struct CrateNameDoesNotMatch {
    #[primary_span]
    pub span: Span,
    pub s: Symbol,
    pub name: Symbol,
}

#[derive(Diagnostic)]
#[diag(session_crate_name_invalid)]
pub struct CrateNameInvalid<'a> {
    pub s: &'a str,
}

#[derive(Diagnostic)]
#[diag(session_crate_name_empty)]
pub struct CrateNameEmpty {
    #[primary_span]
    pub span: Option<Span>,
}

#[derive(Diagnostic)]
#[diag(session_invalid_character_in_create_name)]
pub struct InvalidCharacterInCrateName {
    #[primary_span]
    pub span: Option<Span>,
    pub character: char,
    pub crate_name: Symbol,
}

#[derive(Subdiagnostic)]
#[multipart_suggestion(session_expr_parentheses_needed, applicability = "machine-applicable")]
pub struct ExprParenthesesNeeded {
    #[suggestion_part(code = "(")]
    pub left: Span,
    #[suggestion_part(code = ")")]
    pub right: Span,
}

impl ExprParenthesesNeeded {
    pub fn surrounding(s: Span) -> Self {
        ExprParenthesesNeeded { left: s.shrink_to_lo(), right: s.shrink_to_hi() }
    }
}

#[derive(Diagnostic)]
#[diag(session_skipping_const_checks)]
pub struct SkippingConstChecks {
    #[subdiagnostic(eager)]
    pub unleashed_features: Vec<UnleashedFeatureHelp>,
}

#[derive(Subdiagnostic)]
pub enum UnleashedFeatureHelp {
    #[help(session_unleashed_feature_help_named)]
    Named {
        #[primary_span]
        span: Span,
        gate: Symbol,
    },
    #[help(session_unleashed_feature_help_unnamed)]
    Unnamed {
        #[primary_span]
        span: Span,
    },
}

#[derive(Diagnostic)]
#[diag(session_invalid_literal_suffix)]
pub(crate) struct InvalidLiteralSuffix<'a> {
    #[primary_span]
    #[label]
    pub span: Span,
    // FIXME(#100717)
    pub kind: &'a str,
    pub suffix: Symbol,
}

#[derive(Diagnostic)]
#[diag(session_invalid_int_literal_width)]
#[help]
pub(crate) struct InvalidIntLiteralWidth {
    #[primary_span]
    pub span: Span,
    pub width: String,
}

#[derive(Diagnostic)]
#[diag(session_invalid_num_literal_base_prefix)]
#[note]
pub(crate) struct InvalidNumLiteralBasePrefix {
    #[primary_span]
    #[suggestion(applicability = "maybe-incorrect", code = "{fixed}")]
    pub span: Span,
    pub fixed: String,
}

#[derive(Diagnostic)]
#[diag(session_invalid_num_literal_suffix)]
#[help]
pub(crate) struct InvalidNumLiteralSuffix {
    #[primary_span]
    #[label]
    pub span: Span,
    pub suffix: String,
}

#[derive(Diagnostic)]
#[diag(session_invalid_float_literal_width)]
#[help]
pub(crate) struct InvalidFloatLiteralWidth {
    #[primary_span]
    pub span: Span,
    pub width: String,
}

#[derive(Diagnostic)]
#[diag(session_invalid_float_literal_suffix)]
#[help]
pub(crate) struct InvalidFloatLiteralSuffix {
    #[primary_span]
    #[label]
    pub span: Span,
    pub suffix: String,
}

#[derive(Diagnostic)]
#[diag(session_int_literal_too_large)]
pub(crate) struct IntLiteralTooLarge {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(session_hexadecimal_float_literal_not_supported)]
pub(crate) struct HexadecimalFloatLiteralNotSupported {
    #[primary_span]
    #[label(session_not_supported)]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(session_octal_float_literal_not_supported)]
pub(crate) struct OctalFloatLiteralNotSupported {
    #[primary_span]
    #[label(session_not_supported)]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(session_binary_float_literal_not_supported)]
pub(crate) struct BinaryFloatLiteralNotSupported {
    #[primary_span]
    #[label(session_not_supported)]
    pub span: Span,
}

pub fn report_lit_error(sess: &ParseSess, err: LitError, lit: token::Lit, span: Span) {
    // Checks if `s` looks like i32 or u1234 etc.
    fn looks_like_width_suffix(first_chars: &[char], s: &str) -> bool {
        s.len() > 1 && s.starts_with(first_chars) && s[1..].chars().all(|c| c.is_ascii_digit())
    }

    // Try to lowercase the prefix if it's a valid base prefix.
    fn fix_base_capitalisation(s: &str) -> Option<String> {
        if let Some(stripped) = s.strip_prefix('B') {
            Some(format!("0b{stripped}"))
        } else if let Some(stripped) = s.strip_prefix('O') {
            Some(format!("0o{stripped}"))
        } else if let Some(stripped) = s.strip_prefix('X') {
            Some(format!("0x{stripped}"))
        } else {
            None
        }
    }

    let token::Lit { kind, suffix, .. } = lit;
    match err {
        // `LexerError` is an error, but it was already reported
        // by lexer, so here we don't report it the second time.
        LitError::LexerError => {}
        LitError::InvalidSuffix => {
            if let Some(suffix) = suffix {
                sess.emit_err(InvalidLiteralSuffix { span, kind: kind.descr(), suffix });
            }
        }
        LitError::InvalidIntSuffix => {
            let suf = suffix.expect("suffix error with no suffix");
            let suf = suf.as_str();
            if looks_like_width_suffix(&['i', 'u'], suf) {
                // If it looks like a width, try to be helpful.
                sess.emit_err(InvalidIntLiteralWidth { span, width: suf[1..].into() });
            } else if let Some(fixed) = fix_base_capitalisation(suf) {
                sess.emit_err(InvalidNumLiteralBasePrefix { span, fixed });
            } else {
                sess.emit_err(InvalidNumLiteralSuffix { span, suffix: suf.to_string() });
            }
        }
        LitError::InvalidFloatSuffix => {
            let suf = suffix.expect("suffix error with no suffix");
            let suf = suf.as_str();
            if looks_like_width_suffix(&['f'], suf) {
                // If it looks like a width, try to be helpful.
                sess.emit_err(InvalidFloatLiteralWidth { span, width: suf[1..].to_string() });
            } else {
                sess.emit_err(InvalidFloatLiteralSuffix { span, suffix: suf.to_string() });
            }
        }
        LitError::NonDecimalFloat(base) => {
            match base {
                16 => sess.emit_err(HexadecimalFloatLiteralNotSupported { span }),
                8 => sess.emit_err(OctalFloatLiteralNotSupported { span }),
                2 => sess.emit_err(BinaryFloatLiteralNotSupported { span }),
                _ => unreachable!(),
            };
        }
        LitError::IntTooLarge => {
            sess.emit_err(IntLiteralTooLarge { span });
        }
    }
}
