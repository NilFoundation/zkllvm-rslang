use crate::spec::{LinkerFlavor, LinkerFlavorCli, Target, TargetOptions};

fn options() -> TargetOptions {
    let mut pre_link_args = TargetOptions::link_args(LinkerFlavor::LlvmLink, &[""]);

    // We want to emit .ll file
    super::add_link_args(&mut pre_link_args, LinkerFlavor::LlvmLink, &["-S"]);

    TargetOptions {
        os: "unknown".into(),

        linker: Some("llvm-link".into()),
        linker_flavor: LinkerFlavor::LlvmLink,
        linker_flavor_json: LinkerFlavorCli::LlvmIrLinker,
        linker_is_gnu_json: false,

        pre_link_args,

        ..Default::default()
    }
}

pub fn target() -> Target {
    let options = options();

    Target {
        llvm_target: "assigner-unknown-unknown".into(),
        data_layout: "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128".into(),
        pointer_width: 64,
        arch: "assigner".into(),
        options,
    }
}
