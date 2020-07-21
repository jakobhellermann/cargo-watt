use crate::{
    utils,
    wasm::{self, ProcMacroFn, ProcMacroKind},
    CompilationOptions,
};
use std::path::{Path, PathBuf};

pub fn build(
    directory: &Path,
    compilation_options: &CompilationOptions,
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

    let (fns, wasm) = wasm::compile(directory, &manifest, compilation_options)?;

    create_watt_crate(
        manifest,
        &wasm,
        fns,
        &crate_path,
        directory,
        only_copy_essential,
        compilation_options.compress,
    )?;

    Ok(())
}

// Replaces the [dependency] section with a `watt = "0.4"` dependency
fn modify_cargo_toml_for_watt(manifest: &mut toml_edit::Document, compress: bool) {
    // if the crate depends on proc-macro-hack, we wanna use it aswell
    let proc_macro_hack = manifest["dependencies"]["proc-macro-hack"].clone();

    manifest.as_table_mut().remove("dependencies");

    let mut deps = toml_edit::Table::default();
    deps["watt"] = toml_edit::value("0.4");
    deps["proc-macro-hack"] = proc_macro_hack;

    if compress {
        deps["miniz_oxide"] = toml_edit::value("0.3");
        deps["once_cell"] = toml_edit::value("1.4");
    }

    manifest["dependencies"] = toml_edit::Item::Table(deps);

    if !manifest["features"].is_none() {
        log::warn!(
            "features aren't supported in watt, the crate will be compiled with all enabled"
        );
        let table = manifest["features"].as_table_mut();
        if let Some(features) = table {
            let all_features: Vec<String> = features.iter().map(|(f, _)| f.to_string()).collect();
            for f in all_features {
                features[&f] = toml_edit::value(toml_edit::Array::default());
            }
        }
    }
}

fn watt_librs(name: &str, fns: &[ProcMacroFn], compress: bool) -> String {
    let uses_proc_macro_hack = fns.iter().any(|f| f.kind == ProcMacroKind::ProcMacroHack);
    let use_proc_macro_hack = if uses_proc_macro_hack {
        Some(quote::quote! { use proc_macro_hack::proc_macro_hack; })
    } else {
        None
    };

    let mut file_name = format!("{}.wasm", &name);
    if compress {
        file_name.push_str(".deflate");
    }

    let macro_static = if compress {
        quote::quote! {
            extern crate once_cell;

            use once_cell::sync::Lazy;

            static WASM: Lazy<Vec<u8>> = Lazy::new(|| miniz_oxide::inflate::decompress_to_vec(include_bytes!(#file_name)).expect("failed to decomress wasm"));
            static MACRO: Lazy<watt::WasmMacro> = Lazy::new(|| watt::WasmMacro::new(&WASM));
        }
    } else {
        quote::quote! {
            static WASM: &[u8] = include_bytes!(#file_name);
            static MACRO: watt::WasmMacro = watt::WasmMacro::new(WASM);
        }
    };

    let lib = quote::quote! {
        #macro_static
        #use_proc_macro_hack

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
    compress: bool,
) -> Result<(), anyhow::Error> {
    let name = manifest["package"]["name"].as_str().unwrap().to_string();

    modify_cargo_toml_for_watt(&mut manifest, compress);
    let new_toml = manifest.to_string_in_original_order();
    let lib = watt_librs(&name, &fns, compress);

    let src = crate_path.join("src");

    if !only_copy_essential {
        utils::copy_all(tmp_directory, &crate_path)?;
        std::fs::remove_file(crate_path.join("Cargo.lock"))?;
        std::fs::remove_dir_all(&src)?;
    }

    let mut wasm_file = src.join(&name).with_extension("wasm");
    if compress {
        wasm_file.set_extension("wasm.deflate");
    }

    std::fs::create_dir_all(&src)?;
    std::fs::write(crate_path.join("Cargo.toml"), new_toml)?;
    std::fs::write(wasm_file, wasm)?;
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
