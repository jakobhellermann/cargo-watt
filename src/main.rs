mod modifications;
mod utils;

use anyhow::Context;
use clap::Clap;
use modifications::ProcMacroFn;
use std::{path::PathBuf, process::Command};

#[derive(Debug, Clap)]
struct Options {
    #[clap(default_value = ".")]
    path: PathBuf,
}

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "cargo_watt=info");
    }
    pretty_env_logger::init();

    let options = Options::parse();
    if let Err(e) = run(options) {
        log::error!("{:?}", e);
        std::process::exit(1);
    }
}

fn run(options: Options) -> Result<(), anyhow::Error> {
    let manifest = utils::parse_validate_toml(&options.path.join("Cargo.toml"))?;

    let (fns, wasm) = build_wasm(&options, &manifest)?;
    log::info!("compiled wasm file is {}mb large", wasm.len() / 1024 / 1024);

    create_watt_crate(manifest, &wasm, fns)?;

    Ok(())
}

/// First `build_wasm` copies the crate into /tmp/$crate_name so that I dont fuck something up.
/// Then modify Cargo.toml (proc-macro2 patch, cdylib) and lib.rs (see modifications::librs).
/// Next call cargo build --release --target wasm32-unknown-unknown and read to compiled wasm file.
fn build_wasm(
    options: &Options,
    manifest: &toml_edit::Document,
) -> Result<(Vec<ProcMacroFn>, Vec<u8>), anyhow::Error> {
    let name = manifest["package"]["name"]
        .as_str()
        .ok_or(anyhow::anyhow!("crate has no name"))?;

    let tempdir = std::env::temp_dir().join(name);
    utils::copy_all(&options.path, &tempdir).context("failed to copy to tmp dir")?;

    let fns = modifications::make_modifications(&tempdir)
        .context("failed to make modifications to crate")?;

    log::info!("begin compiling crate...");
    let instant = std::time::Instant::now();
    let status = Command::new("cargo")
        .args(&["build", "--target", "wasm32-unknown-unknown", "--release"])
        .current_dir(&tempdir)
        .status()
        .context("failed to run cargo build")?;
    log::info!("finished in {:.1}s", instant.elapsed().as_secs_f32());
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
    mut manifest: toml_edit::Document,
    wasm: &[u8],
    fns: Vec<ProcMacroFn>,
) -> Result<(), anyhow::Error> {
    let name = manifest["package"]["name"].as_str().unwrap().to_string();

    let crate_path = PathBuf::from(format!("{}-watt", name));
    let src_path = crate_path.join("src");

    std::fs::create_dir_all(&src_path)?;

    std::fs::write(src_path.join(&name).with_extension("wasm"), wasm)?;

    manifest.as_table_mut().remove("dependencies");
    let mut deps = toml_edit::Table::default();
    deps["watt"] = toml_edit::value("0.3");
    manifest
        .as_table_mut()
        .entry("dependencies")
        .or_insert(toml_edit::Item::Table(deps));

    std::fs::write(
        crate_path.join("Cargo.toml"),
        manifest.to_string_in_original_order(),
    )?;

    let file_name = format!("{}.wasm", &name);
    let lib = quote::quote! {
        static MACRO: watt::WasmMacro = watt::WasmMacro::new(WASM);
        static WASM: &[u8] = include_bytes!(#file_name);

        #(#fns)*
    };
    std::fs::write(src_path.join("lib.rs"), lib.to_string())?;

    log::info!("generated crate in {:?}", crate_path);

    Ok(())
}
