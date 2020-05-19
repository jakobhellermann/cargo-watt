mod modifications;
mod utils;

use anyhow::Context;
use clap::Clap;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/*macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}*/

#[derive(Debug, Clap)]
struct Options {
    path: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let options = Options::parse();

    anyhow::ensure!(options.path.is_dir(), "no Cargo.toml found");
    anyhow::ensure!(
        options.path.join("Cargo.toml").exists(),
        "no Cargo.toml found"
    );

    let tempdir = std::env::temp_dir().join(options.path.file_name().unwrap());
    utils::copy_all(&options.path, &tempdir).context("failed to copy to tmp dir")?;

    make_modifications(&tempdir).context("failed to make modifications to crate")?;

    let status = Command::new("/home/jakob/.cargo/bin/cargo")
        .args(&["build", "--target", "wasm32-unknown-unknown"])
        .current_dir(&tempdir)
        .status()
        .context("failed to run cargo build")?;
    anyhow::ensure!(status.success(), "cargo failed");

    let dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| tempdir.join("target"))
        .join("wasm32-unknown-unknown/release");
    println!("{}", dir.display());

    std::fs::remove_dir_all(tempdir)?;

    Ok(())
}

fn make_modifications(path: &Path) -> Result<(), anyhow::Error> {
    let toml_path = path.join("Cargo.toml");
    let toml = std::fs::read_to_string(&toml_path)?;
    let new_toml = modifications::cargo_toml(&toml)?;
    std::fs::write(toml_path, new_toml)?;

    let lib_path = path.join("src").join("lib.rs");
    let lib = std::fs::read_to_string(&lib_path)?;
    let new_lib = modifications::librs(&lib)?;
    std::fs::write(lib_path, new_lib)?;

    Ok(())
}
