use crate::spec::{Target, TargetOptions};

pub fn target() -> Target {
    let mut options = TargetOptions::default();
    // TODO: (aleasims) describe options carefully.
    options.is_builtin = true;
    Target {
        llvm_target: "assigner-unknown-unknown".into(),
        data_layout: "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128".into(),
        pointer_width: 64,
        arch: "assigner".into(),
        options,
    }
}
