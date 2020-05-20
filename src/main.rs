mod modifications;
mod utils;

use anyhow::Context;
use clap::Clap;
use modifications::ProcMacroFn;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clap)]
struct Options {
    #[clap(default_value = ".")]
    path: PathBuf,

    #[clap(long, conflicts_with = "path")]
    git: Option<String>,

    #[cfg_attr(not(feature = "crates"), clap(hidden = true))]
    #[clap(long = "crate", conflicts_with = "path", conflicts_with = "git")]
    crate_: Option<String>,

    #[clap(long)]
    only_copy_essential: bool,

    #[clap(long)]
    overwrite: bool,
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
    let tempdir = std::env::temp_dir().join("cargo-watt-crate");
    if tempdir.exists() {
        std::fs::remove_dir_all(&tempdir)?;
    }
    std::fs::create_dir_all(&tempdir)?;

    // copy crate (local directory, crates.io, git) into /tmp/cargo-watt-crate
    if let Some(git) = &options.git {
        log::info!("git clone '{}' into temporary directory...", &git);
        utils::clone_git_into(&tempdir, git)?;
    } else if let Some(crate_) = &options.crate_ {
        log::info!("download crate '{}' into temporary directory...", crate_);
        #[cfg(feature = "crates")]
        utils::download_crate(&tempdir, crate_).context("failed to download and extract crate")?;
        #[cfg(not(feature = "crates"))]
        panic!("the crate was compiled without the 'crates' feature flag");
    } else {
        utils::copy_all(&options.path, &tempdir).context("failed to copy to tmp dir")?;
    }

    let manifest = utils::parse_validate_toml(&tempdir.join("Cargo.toml"))
        .context("failed to parse Cargo.toml")?;
    let name = manifest["package"]["name"].as_str().unwrap().to_string();
    let crate_path = PathBuf::from(format!("{}-watt", name));
    match (crate_path.exists(), options.overwrite) {
        (true, false) => anyhow::bail!("'{}' already exists. Use --overwrite to overwrite.", crate_path.display()),
        (true, true) => std::fs::remove_dir_all(&crate_path)?,
        (false, _) => {},
    }

    let (fns, wasm) = build_wasm(&tempdir, &manifest)?;
    let size_in_mb = wasm.len() as f32 / 1024.0 / 1024.0;
    log::info!("compiled wasm file is {:.2}mb large", size_in_mb);

    create_watt_crate(
        manifest,
        &wasm,
        fns,
        &crate_path,
        &tempdir,
        options.only_copy_essential,
    )?;

    std::fs::remove_dir_all(&tempdir)?;

    Ok(())
}

/// First `build_wasm` copies the crate into /tmp/$crate_name so that I dont fuck something up.
/// Then modify Cargo.toml (proc-macro2 patch, cdylib) and lib.rs (see modifications::librs).
/// Next call cargo build --release --target wasm32-unknown-unknown and read to compiled wasm file.
fn build_wasm(
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

/// Replaces the [dependency] section with a `watt = "0.3"` dependency
fn modify_cargo_toml_for_watt(manifest: &mut toml_edit::Document) {
    manifest.as_table_mut().remove("dependencies");
    let mut deps = toml_edit::Table::default();
    deps["watt"] = toml_edit::value("0.3");
    manifest
        .as_table_mut()
        .entry("dependencies")
        .or_insert(toml_edit::Item::Table(deps));
}

fn watt_librs(name: &str, fns: &[ProcMacroFn]) -> String {
    let file_name = format!("{}.wasm", &name);
    let lib = quote::quote! {
        static MACRO: watt::WasmMacro = watt::WasmMacro::new(WASM);
        static WASM: &[u8] = include_bytes!(#file_name);

        #(#fns)*
    };

    lib.to_string()
}

fn create_watt_crate(
    mut manifest: toml_edit::Document,
    wasm: &[u8],
    fns: Vec<ProcMacroFn>,
    crate_path: &Path,
    tmp_directory: &Path,
    only_copy_essential: bool,
) -> Result<(), anyhow::Error> {
    let name = manifest["package"]["name"].as_str().unwrap().to_string();

    modify_cargo_toml_for_watt(&mut manifest);
    let new_toml = manifest.to_string_in_original_order();
    let lib = watt_librs(&name, &fns);

    let src = crate_path.join("src");

    if !only_copy_essential {
        utils::copy_all(tmp_directory, &crate_path)?;
        std::fs::remove_dir_all(&src)?;
    }

    std::fs::create_dir_all(&src)?;
    std::fs::write(crate_path.join("Cargo.toml"), new_toml)?;
    std::fs::write(src.join(&name).with_extension("wasm"), wasm)?;
    std::fs::write(src.join("lib.rs"), lib.to_string())?;

    log::info!("generated crate in {:?}", crate_path);

    if let Err(e) = utils::cargo_fmt(&crate_path) {
        log::warn!("failed to format crate: {}", e);
    }

    Ok(())
}
