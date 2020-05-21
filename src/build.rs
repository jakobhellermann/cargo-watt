use crate::{
    utils,
    wasm::{self, ProcMacroFn},
};
use std::path::{Path, PathBuf};

pub fn build(
    directory: &Path,
    only_copy_essential: bool,
    overwrite: bool,
) -> Result<(), anyhow::Error> {
    let manifest = utils::parse_validate_toml(&directory.join("Cargo.toml"))?;
    let name = manifest["package"]["name"].as_str().unwrap().to_string();
    let crate_path = PathBuf::from(format!("{}-watt", name));
    match (crate_path.exists(), overwrite) {
        (true, false) => anyhow::bail!(
            "'{}' already exists. Use --overwrite to overwrite.",
            crate_path.display()
        ),
        (true, true) => std::fs::remove_dir_all(&crate_path)?,
        (false, _) => {}
    }

    let (fns, wasm) = wasm::compile(directory, &manifest)?;
    let size_in_mb = wasm.len() as f32 / 1024.0 / 1024.0;
    log::info!("compiled wasm file is {:.2}mb large", size_in_mb);

    create_watt_crate(
        manifest,
        &wasm,
        fns,
        &crate_path,
        directory,
        only_copy_essential,
    )?;

    Ok(())
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
        std::fs::remove_file(crate_path.join("Cargo.lock"))?;
        std::fs::remove_dir_all(&src)?;
    }

    std::fs::create_dir_all(&src)?;
    std::fs::write(crate_path.join("Cargo.toml"), new_toml)?;
    std::fs::write(src.join(&name).with_extension("wasm"), wasm)?;
    std::fs::write(src.join("lib.rs"), lib.to_string())?;

    std::fs::rename(
        tmp_directory.join("Cargo.lock"),
        crate_path.join("Cargo.watt.lock"),
    )?;

    log::info!("generated crate in {:?}", crate_path);

    if let Err(e) = utils::cargo_fmt(&crate_path) {
        log::warn!("failed to format crate: {}", e);
    }

    Ok(())
}
