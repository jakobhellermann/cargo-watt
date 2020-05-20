use anyhow::Context;
use std::{path::Path, process::Command};
use walkdir::WalkDir;

pub fn parse_validate_toml(path: &Path) -> Result<toml_edit::Document, anyhow::Error> {
    let input = std::fs::read_to_string(path)?;
    let manifest: toml_edit::Document = input.parse()?;

    anyhow::ensure!(!manifest["package"].is_none(), "Cargo.toml has no package");
    anyhow::ensure!(!manifest["lib"].is_none(), "Cargo.toml has no lib");
    anyhow::ensure!(
        manifest["lib"]["proc-macro"].as_bool().unwrap_or(false),
        "crate is not a proc macro"
    );

    Ok(manifest)
}

pub fn copy_all(from: &Path, to: &Path) -> Result<(), anyhow::Error> {
    anyhow::ensure!(from.is_dir(), "from path should be a directory");
    if to.exists() {
        std::fs::remove_dir_all(&to)?;
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let len = from.components().fold(0, |acc, _| acc + 1);

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
            .skip(len)
            .fold(to.to_path_buf(), |acc, item| acc.join(item));
        if file_type.is_dir() {
            std::fs::create_dir(new_file)?;
        } else {
            std::fs::copy(entry.path(), new_file)?;
        }
    }

    Ok(())
}

pub fn clone_git_into(path: &Path, url: &str) -> Result<(), anyhow::Error> {
    let output = Command::new("git")
        .arg("clone")
        .arg(url)
        .arg(path)
        .output()
        .context("cannot execute git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to clone {}: {}", url, stderr);
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
