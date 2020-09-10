use crate::CompilationOptions;
use anyhow::Context;
use cargo_metadata::{CargoOpt, MetadataCommand, Package};
use std::path::Path;

const WATT_DIR: &str = ".watt-patched";

fn is_proc_macro(package: &Package) -> bool {
    package
        .targets
        .iter()
        .any(|target| target.kind.iter().any(|kind| kind == "proc-macro"))
}

pub fn add_patches(toml_path: &Path, patches: &[&str]) -> Result<(), anyhow::Error> {
    let input = std::fs::read_to_string(&toml_path)?;
    let mut manifest: toml_edit::Document = input.parse()?;

    let patch = crate::utils_toml::implicit_table(&mut manifest, "patch", "crates-io");
    for name in patches {
        let path_str = format!("./{}/{}", WATT_DIR, name);
        patch[name] = toml_edit::value(crate::utils_toml::dependency("path", &path_str));
    }

    let new_toml = manifest.to_string_in_original_order();
    std::fs::write(toml_path, new_toml)?;

    Ok(())
}

pub fn patch(path: &Path, compilation_options: &CompilationOptions) -> Result<(), anyhow::Error> {
    let watt_crate_dir = path.join(WATT_DIR);

    let metadata = MetadataCommand::new()
        .current_dir(&path)
        .features(CargoOpt::AllFeatures)
        .exec()?;

    let patched_deps: Vec<&str> = metadata
        .packages
        .iter()
        .filter(|package| is_proc_macro(package))
        .map(|package: &Package| -> Result<_, anyhow::Error> {
            let crate_path = watt_crate_dir.join(&package.name);

            let input = crate::Input::crate_(package.name.clone());
            let tempdir = input.in_tempdir()?;

            crate::build::build(
                &tempdir,
                Some(crate_path.clone()),
                &compilation_options,
                true,
                true,
            )
            .with_context(|| format!("failed to build crate {}", package.name))?;

            Ok(package.name.as_str())
        })
        .collect::<Result<_, _>>()?;

    add_patches(&path.join("Cargo.toml"), &patched_deps)?;

    Ok(())
}
