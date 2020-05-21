mod modifications;

pub use modifications::ProcMacroFn;

use anyhow::Context;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Modify Cargo.toml (proc-macro2 patch, cdylib) and lib.rs (see modifications::librs).
/// Then call cargo build --release --target wasm32-unknown-unknown and read to compiled wasm file.
pub fn compile(
    directory: &Path,
    manifest: &toml_edit::Document,
) -> Result<(Vec<ProcMacroFn>, Vec<u8>), anyhow::Error> {
    let name = manifest["package"]["name"].as_str().unwrap();

    let fns = modifications::make_modifications(&directory)
        .context("failed to make modifications to crate")?;

    log::info!("begin compiling crate...");
    let instant = std::time::Instant::now();
    let status = Command::new("cargo")
        .args(&["build", "--target", "wasm32-unknown-unknown", "--release"])
        .current_dir(&directory)
        .status()
        .context("failed to run cargo build")?;
    log::info!("finished in {:.1}s", instant.elapsed().as_secs_f32());
    anyhow::ensure!(status.success(), "cargo failed");

    let wasm_path = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| directory.join("target"))
        .join("wasm32-unknown-unknown/release")
        .join(name.replace("-", "_"))
        .with_extension("wasm");

    let wasm = std::fs::read(wasm_path).context("cannot read compiled wasm")?;

    Ok((fns, wasm))
}
