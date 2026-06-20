fn main() {
    // Embed an application manifest (asInvoker) into the setup binary so
    // Windows' installer-detection heuristics don't auto-elevate an exe named
    // setup.exe (which would break stdout and our targeted elevation step).
    // Applies to binaries only; the cdylib is unaffected.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_manifest::embed_manifest(embed_manifest::new_manifest("AzureSapi.Setup"))
            .expect("embedding manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
