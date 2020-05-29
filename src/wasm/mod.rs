mod modifications;

pub use modifications::{ProcMacroFn, ProcMacroKind};

use crate::CompilationOptions;
use anyhow::Context;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
fn file_size(path: &Path) -> Result<u64, std::io::Error> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(&path)?.size())
}
#[cfg(not(unix))]
fn file_size(path: &Path) -> Result<u64, anyhow::Error> {
    let content = std::fs::read(path)?;
    Ok(content.len() as u64)
}

/// Modify Cargo.toml (proc-macro2 patch, cdylib) and lib.rs (see modifications::librs).
/// Then call cargo build --release --target wasm32-unknown-unknown and read to compiled wasm file.
pub fn compile(
    directory: &Path,
    manifest: &toml_edit::Document,
    compilation_options: &CompilationOptions,
) -> Result<(Vec<ProcMacroFn>, Vec<u8>), anyhow::Error> {
    let name = manifest["package"]["name"].as_str().unwrap();

    let fns = modifications::make_modifications(&directory)
        .context("failed to make modifications to crate")?;

    log::info!("begin compiling crate...");
    let instant = std::time::Instant::now();
    let status = Command::new("cargo")
        .args(&[
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
            "--all-features",
        ])
        .env("RUSTFLAGS", rust_flags())
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

    let size = file_size(&wasm_path)?;
    log::debug!("wasm file size: {}kb", size / 1024);

    if !compilation_options.no_wasm_strip {
        let status = Command::new("wasm-strip")
            .arg(&wasm_path)
            .status()
            .context("failed to run wasm-strip")?;
        anyhow::ensure!(status.success(), "wasm-strip failed");

        let size = file_size(&wasm_path)?;
        log::debug!("after wasm-strip: {}kb", size / 1024);
    }
    if !compilation_options.no_wasm_opt {
        let status = Command::new("wasm-opt")
            .arg(&wasm_path)
            .arg("-o")
            .arg(&wasm_path)
            .arg("-Os")
            .status()
            .context("failed to run wasm-opt")?;
        anyhow::ensure!(status.success(), "wasm-opt failed");

        let size = file_size(&wasm_path)?;
        log::debug!("after wasm-opt: {}kb", size / 1024);
    }

    let wasm = std::fs::read(wasm_path).context("cannot read compiled wasm")?;

    Ok((fns, wasm))
}

fn rust_flags() -> String {
    match std::env::var("CARGO_HOME") {
        Ok(cargo_home) => format!("--remap-path-prefix {}=/cargo_home", cargo_home),
        Err(_) => {
            log::warn!("the $CARGO_HOME environment variable is not set, probably because you didn't run this as a subcommand.
 The compiled wasm file will include paths to your local cargo installation, making it hard to ensure reproducible builds.");
            "".into()
        }
    }
}
