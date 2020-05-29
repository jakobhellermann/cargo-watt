use crate::{utils, wasm, CompilationOptions};
use std::path::Path;

pub fn verify(
    directory: &Path,
    compilation_options: &CompilationOptions,
    wasm_file: &Path,
) -> Result<(), anyhow::Error> {
    let is_wasm = wasm_file.extension().map_or(false, |e| e == "wasm");
    anyhow::ensure!(is_wasm, "'{}' is not a wasm file", wasm_file.display());

    let wasm = std::fs::read(wasm_file)?;

    let manifest = utils::parse_validate_toml(&directory.join("Cargo.toml"))?;
    let name = manifest["package"]["name"].as_str().unwrap();
    let (_, compiled_wasm) = wasm::compile(directory, &manifest, compilation_options)?;

    if wasm != compiled_wasm {
        let file_name = wasm_file.file_name().unwrap().to_str().unwrap();
        anyhow::bail!(
            "'{}' wasn't compiled from '{}' or the build wasn't reproducible",
            &file_name,
            name
        );
    }

    eprintln!(" Success!");

    Ok(())
}
