use anyhow::Context;
use std::{path::Path, process::Command};
use walkdir::WalkDir;

pub fn parse_validate_toml(path: &Path) -> Result<toml_edit::Document, anyhow::Error> {
    let input = std::fs::read_to_string(path).context("error reading Cargo.toml")?;
    let manifest: toml_edit::Document = input.parse().context("failed to parse Cargo.toml")?;

    anyhow::ensure!(!manifest["package"].is_none(), "Cargo.toml has no package");
    anyhow::ensure!(
        manifest["package"]["name"].as_str().is_some(),
        "Cargo.toml has no name"
    );
    anyhow::ensure!(
        manifest["lib"]["proc-macro"].as_bool().unwrap_or(false),
        "crate is not a proc macro"
    );
    anyhow::ensure!(
        manifest["dependencies"]["watt"].is_none(),
        "already a 'watt' crate"
    );

    Ok(manifest)
}

pub fn copy_all(from: &Path, to: &Path) -> Result<(), anyhow::Error> {
    anyhow::ensure!(from.is_dir(), "'{}' is not a directory", from.display());

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
        if file_type.is_dir() && !new_file.exists() {
            std::fs::create_dir(new_file)?;
        } else if file_type.is_file() {
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
        anyhow::bail!("failed to clone {}: {}", url, stderr.trim());
    }
    Ok(())
}

pub fn cargo_fmt(path: &Path) -> Result<(), anyhow::Error> {
    let output = Command::new("cargo")
        .arg("fmt")
        .current_dir(path)
        .output()
        .context("cannot execute git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{}", stderr);
    }
    Ok(())
}

#[cfg(feature = "crates")]
pub fn download_crate(path: &Path, crate_: &str) -> Result<(), anyhow::Error> {
    let err = |e| move || anyhow::anyhow!("invalid crates.io response: {}", e);

    let response = ureq::get(&format!("https://crates.io/api/v1/crates/{}", crate_)).call();
    anyhow::ensure!(
        !response.error(),
        "crates io request failed with status code {}: {}",
        response.status(),
        response.status_text()
    );
    let body = response.into_reader();
    let api_response: serde_json::Value = serde_json::from_reader(body)?;

    let versions = &api_response["versions"]
        .as_array()
        .ok_or_else(err("no versions"))?;
    let dl_path = versions
        .iter()
        .filter(|v| !v["yanked"].as_bool().unwrap_or(false))
        .next()
        .ok_or_else(|| anyhow::anyhow!("no published non-yanked versions"))?["dl_path"]
        .as_str()
        .ok_or_else(err("missing dl_path"))?;

    let crate_response = ureq::get(&format!("https://crates.io{}", dl_path)).call();

    anyhow::ensure!(
        !crate_response.error(),
        "crates io request failed with status code {}: {}",
        crate_response.status(),
        crate_response.status_text()
    );

    let tar = flate2::read::GzDecoder::new(crate_response.into_reader());
    let mut archive = tar::Archive::new(tar);
    archive.unpack(path)?;

    for file in std::fs::read_dir(path)? {
        let inner_path = file?.path();
        copy_all(&inner_path, path)?;
        std::fs::remove_dir_all(&inner_path)?;
    }

    // if we don't delete Cargo.lock, the #[patch] will not be used
    let lock = path.join("Cargo.lock");
    if lock.exists() {
        std::fs::remove_file(path.join("Cargo.lock"))?;
    }

    Ok(())
}
