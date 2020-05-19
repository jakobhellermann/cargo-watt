mod modifications;
mod utils;

use anyhow::Context;
use clap::Clap;
use std::{path::PathBuf, process::Command};

#[derive(Debug, Clap)]
struct Options {
    #[clap(default_value = ".")]
    path: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let options = Options::parse();
    let manifest = utils::parse_validate_toml(&options.path.join("Cargo.toml"))?;

    let wasm = build_wasm(&options, &manifest)?;
    println!("{}kb", wasm.len() / 1024);

    Ok(())
}

fn build_wasm(
    options: &Options,
    manifest: &cargo_toml::Manifest,
) -> Result<Vec<u8>, anyhow::Error> {
    let name = manifest.package.as_ref().unwrap().name.as_str();

    let tempdir = std::env::temp_dir().join(name);
    utils::copy_all(&options.path, &tempdir).context("failed to copy to tmp dir")?;

    modifications::make_modifications(&tempdir).context("failed to make modifications to crate")?;

    let status = Command::new("/home/jakob/.cargo/bin/cargo")
        .args(&["build", "--target", "wasm32-unknown-unknown", "--release"])
        .current_dir(&tempdir)
        .status()
        .context("failed to run cargo build")?;
    anyhow::ensure!(status.success(), "cargo failed");

    let wasm_path = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| tempdir.join("target"))
        .join("wasm32-unknown-unknown/release")
        .join(name.replace("-", "_"))
        .with_extension("wasm");

    let wasm = std::fs::read(wasm_path).context("cannot read compiled wasm")?;

    std::fs::remove_dir_all(tempdir)?;

    Ok(wasm)
}
