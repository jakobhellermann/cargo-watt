use anyhow::Context;
use std::path::Path;
use walkdir::WalkDir;

pub fn parse_validate_toml(path: &Path) -> Result<cargo_toml::Manifest, anyhow::Error> {
    let toml = std::fs::read(path).context("failed to read Cargo.toml")?;
    let manifest = cargo_toml::Manifest::from_slice(&toml)?;
    anyhow::ensure!(manifest.package.is_some(), "Cargo.toml has no package");
    anyhow::ensure!(manifest.lib.is_some(), "Cargo.toml has no lib");
    anyhow::ensure!(
        manifest.lib.as_ref().unwrap().proc_macro,
        "crate is not a proc macro"
    );

    Ok(manifest)
}

pub fn copy_all(from: &Path, to: &Path) -> Result<(), anyhow::Error> {
    anyhow::ensure!(from.is_dir(), "from path should be a directory");
    if to.exists() {
        std::fs::remove_dir_all(&to)?;
    }

    let files = WalkDir::new(from);
    for file in files {
        let entry = file?;
        let file_type = entry.file_type();

        if file_type.is_symlink() {
            continue;
        }

        let new_file = entry
            .path()
            .components()
            .skip(1)
            .fold(to.to_path_buf(), |acc, item| acc.join(item));
        if file_type.is_dir() {
            std::fs::create_dir(new_file)?;
        } else {
            std::fs::copy(entry.path(), new_file)?;
        }
    }

    Ok(())
}

pub fn parse_attributes(
    token_stream: proc_macro2::TokenStream,
) -> syn::Result<Vec<syn::Attribute>> {
    struct AttrParser(Vec<syn::Attribute>);
    impl syn::parse::Parse for AttrParser {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            Ok(AttrParser(input.call(syn::Attribute::parse_outer)?))
        }
    }

    let AttrParser(attrs) = syn::parse2(token_stream)?;
    Ok(attrs)
}
