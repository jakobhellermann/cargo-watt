mod utils;
mod wasm;

mod build;
mod verify;

use anyhow::Context;
use clap::Clap;
use std::path::PathBuf;

#[derive(Clap, Debug)]
/// Either a path, git repo or crates.io crate
pub struct Input {
    #[clap(default_value = ".")]
    path: PathBuf,

    #[clap(long, conflicts_with = "path")]
    git: Option<String>,

    #[cfg_attr(not(feature = "crates"), clap(hidden = true))]
    #[clap(long = "crate", conflicts_with = "path", conflicts_with = "git")]
    crate_: Option<String>,
}

#[derive(Debug, Clap)]
#[clap(setting = clap::AppSettings::ColoredHelp, bin_name = "cargo watt", about = clap::crate_description!())]
pub enum Options {
    Build {
        #[clap(flatten)]
        input: Input,

        #[clap(long, about = "copy only Cargo.toml and src/* to new crate")]
        only_copy_essential: bool,
        #[clap(long)]
        overwrite: bool,
    },
    Verify {
        #[clap(required = true)]
        file: PathBuf,

        #[clap(flatten)]
        input: Input,
    },
}
impl Options {
    fn input(&self) -> &Input {
        match self {
            Options::Build { input, .. } => input,
            Options::Verify { input, .. } => input,
        }
    }
}

fn main() {
    pretty_env_logger::formatted_builder()
        .parse_filters(&std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let args = std::env::args().filter(|arg| arg != "watt");
    let options = Options::parse_from(args);
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
    let input = options.input();
    if let Some(git) = &input.git {
        log::info!("git clone '{}' into temporary directory...", &git);
        utils::clone_git_into(&tempdir, git)?;
    } else if let Some(crate_) = &input.crate_ {
        log::info!("download crate '{}' into temporary directory...", crate_);
        #[cfg(feature = "crates")]
        utils::download_crate(&tempdir, crate_).context("failed to download and extract crate")?;
        #[cfg(not(feature = "crates"))]
        panic!("the crate was compiled without the 'crates' feature flag");
    } else {
        anyhow::ensure!(
            PathBuf::from("Cargo.toml").exists(),
            "No Cargo.toml found. Use the --git or --crate flag if you want to use a remote crate."
        );
        utils::copy_all(&input.path, &tempdir).context("failed to copy to tmp dir")?;
    }

    match options {
        Options::Build {
            only_copy_essential,
            overwrite,
            ..
        } => build::build(&tempdir, only_copy_essential, overwrite)?,
        Options::Verify { file, .. } => verify::verify(&tempdir, &file)?,
    }

    std::fs::remove_dir_all(&tempdir)?;

    Ok(())
}
