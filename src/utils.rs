use anyhow::Context;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

const UNSUPPORTED_DEPS: &[&str] = &["syn-mid", "synstructure"];

pub fn parse_validate_toml(path: &Path) -> Result<toml_edit::Document, anyhow::Error> {
    let input = std::fs::read_to_string(path).context("error reading Cargo.toml")?;
    let manifest: toml_edit::Document = input.parse().context("failed to parse Cargo.toml")?;

    if manifest["package"]["edition"].as_str() != Some("2018") {
        log::warn!("macro crate is not 2018 edition, which may not work in some cases");
    }

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

    for unsupported in UNSUPPORTED_DEPS {
        let crate_not_there = manifest["dependencies"][unsupported].is_none();
        anyhow::ensure!(
            crate_not_there,
            "crate has dependency on '{}', which doesn't work with cargo watt",
            unsupported
        );
    }

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

pub fn cargo(path: &Path, args: &[&str]) -> Result<(), anyhow::Error> {
    let output = Command::new("cargo")
        .args(args)
        .current_dir(path)
        .output()
        .context("cannot execute cargo")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{}", stderr);
    }
    Ok(())
}

pub fn cargo_fmt(path: &Path) -> Result<(), anyhow::Error> {
    cargo(path, &["fmt"])
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

    for entry in archive.entries()? {
        let mut entry = entry?;

        let path_without_parent: PathBuf = entry.path()?.components().skip(1).collect();
        let new_path = path.join(path_without_parent);

        if let Some(parent) = new_path.parent() {
            std::fs::create_dir_all(&parent)?;
        }

        entry.unpack(new_path)?;
    }

    Ok(())
}

pub struct Tempdir {
    path: PathBuf,
    delete: bool,
}
impl Tempdir {
    pub fn new() -> std::io::Result<Self> {
        let name: String = (0..=6).map(|_| fastrand::alphanumeric()).collect();

        let mut path = std::env::temp_dir();
        path.push(format!(".tmp{}", name));

        if path.exists() {
            std::fs::remove_dir_all(&path)?;
        }
        std::fs::create_dir_all(&path)?;
        Ok(Tempdir { path, delete: true })
    }

    pub fn set_delete(&mut self, delete: bool) {
        self.delete = delete;
    }
}
impl Drop for Tempdir {
    fn drop(&mut self) {
        if self.delete {
            if let Err(e) = std::fs::remove_dir_all(&self.path) {
                log::warn!("failed to delete temporary directory: {}", e);
            }
        }
    }
}
impl std::ops::Deref for Tempdir {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}
