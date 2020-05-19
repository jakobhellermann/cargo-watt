mod modifications;
mod utils;

use anyhow::Context;
use cargo_toml::Manifest;
use clap::Clap;
use modifications::ProcMacroFn;
use std::{path::PathBuf, process::Command};

#[derive(Debug, Clap)]
struct Options {
    #[clap(default_value = ".")]
    path: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let options = Options::parse();
    let manifest = utils::parse_validate_toml(&options.path.join("Cargo.toml"))?;

    let (fns, wasm) = build_wasm(&options, &manifest)?;
    println!("{}kb", wasm.len() / 1024);

    create_watt_crate(&manifest, &wasm, fns)?;

    Ok(())
}

fn build_wasm(
    options: &Options,
    manifest: &Manifest,
) -> Result<(Vec<ProcMacroFn>, Vec<u8>), anyhow::Error> {
    let name = manifest.package.as_ref().unwrap().name.as_str();

    let tempdir = std::env::temp_dir().join(name);
    utils::copy_all(&options.path, &tempdir).context("failed to copy to tmp dir")?;

    let fns = modifications::make_modifications(&tempdir)
        .context("failed to make modifications to crate")?;

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

    Ok((fns, wasm))
}

fn create_watt_crate(
    manifest: &Manifest,
    wasm: &[u8],
    fns: Vec<ProcMacroFn>,
) -> Result<(), anyhow::Error> {
    let name = manifest.package.as_ref().unwrap().name.as_str();
    let crate_path = PathBuf::from(format!("{}-watt", name));
    let src_path = crate_path.join("src");

    std::fs::create_dir_all(&src_path)?;

    std::fs::write(src_path.join(name).with_extension("wasm"), wasm)?;
    std::fs::write(
        crate_path.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{}-watt"
version = "0.1.0"
edition = "2018"

[lib]
proc-macro = true

[dependencies]
watt = "0.3""#,
            name,
        ),
    )?;

    let file_name = format!("{}.wasm", &name);
    let lib = quote::quote! {
        static MACRO: watt::WasmMacro = watt::WasmMacro::new(WASM);
        static WASM: &[u8] = include_bytes!(#file_name);

        #(#fns)*

    };
    std::fs::write(src_path.join("lib.rs"), lib.to_string())?;

    Ok(())
}
