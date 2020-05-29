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

#[derive(Clap, Debug)]
pub struct CompilationOptions {
    #[clap(long)]
    no_wasm_strip: bool,

    #[clap(long)]
    no_wasm_opt: bool,

    #[clap(long)]
    compress: bool,
}
impl CompilationOptions {
    fn verify(&self) -> Result<(), anyhow::Error> {
        let exists = |cmd: &str| {
            std::process::Command::new(cmd)
                .stdout(std::process::Stdio::null())
                .arg("--version")
                .status()
                .is_ok()
        };
        if !self.no_wasm_strip && !exists("wasm-strip") {
            anyhow::bail!("cannot find wasm-strip, try --no-wasm-strip");
        }
        if !self.no_wasm_opt && !exists("wasm-opt") {
            anyhow::bail!("cannot find wasm-opt, try --no-wasm-opt");
        }
        Ok(())
    }
}

#[derive(Debug, Clap)]
#[clap(setting = clap::AppSettings::ColoredHelp, bin_name = "cargo watt", about = clap::crate_description!())]
pub enum Options {
    Build {
        #[clap(flatten)]
        input: Input,

        #[clap(flatten)]
        compilation_options: CompilationOptions,

        #[clap(long, about = "copy only Cargo.toml and src/* to new crate")]
        only_copy_essential: bool,
        #[clap(long)]
        overwrite: bool,

        #[clap(long, about = "don't delete the temporary build directory")]
        keep_tmp: bool,
    },
    Verify {
        #[clap(required = true)]
        file: PathBuf,

        #[clap(flatten)]
        input: Input,

        #[clap(flatten)]
        compilation_options: CompilationOptions,
    },
}
impl Options {
    fn input(&self) -> &Input {
        match self {
            Options::Build { input, .. } => input,
            Options::Verify { input, .. } => input,
        }
    }
    fn compilation_options(&self) -> &CompilationOptions {
        match self {
            Options::Build {
                compilation_options,
                ..
            } => compilation_options,
            Options::Verify {
                compilation_options,
                ..
            } => compilation_options,
        }
    }
    fn keep_tmp(&self) -> bool {
        match self {
            Options::Build { keep_tmp, .. } => *keep_tmp,
            Options::Verify { .. } => false,
        }
    }
}

fn main() {
    pretty_env_logger::formatted_builder()
        .parse_filters(&std::env::var("RUST_LOG").unwrap_or_else(|_| "cargo_watt=debug".into()))
        .init();

    let args = std::env::args().filter(|arg| arg != "watt");
    let options = Options::parse_from(args);
    if let Err(e) = run(options) {
        log::error!("{:?}", e);
        std::process::exit(1);
    }
}

fn run(options: Options) -> Result<(), anyhow::Error> {
    options.compilation_options().verify()?;

    let tempdir = tempfile::tempdir()
        .context("failed to crate temporary directory")?
        .into_path();
    if tempdir.exists() {
        std::fs::remove_dir_all(&tempdir)?;
    }
    std::fs::create_dir_all(&tempdir)?;

    let keep_tmp = options.keep_tmp();
    // if we want to keep the directory, we probably wanna know where it is
    if keep_tmp {
        log::info!("generate temporary directory at '{}'", tempdir.display());
    }

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
        let cargo_toml = input.path.join("Cargo.toml");
        anyhow::ensure!(
            cargo_toml.exists(),
            "No Cargo.toml found. Use the --git or --crate flag if you want to use a remote crate."
        );
        utils::copy_all(&input.path, &tempdir).context("failed to copy to tmp dir")?;
    }

    let result = match options {
        Options::Build {
            only_copy_essential,
            overwrite,
            compilation_options,
            ..
        } => build::build(
            &tempdir,
            &compilation_options,
            only_copy_essential,
            overwrite,
        ),
        Options::Verify {
            file,
            compilation_options,
            ..
        } => verify::verify(&tempdir, &compilation_options, &file),
    };

    if !keep_tmp {
        std::fs::remove_dir_all(tempdir)?;
    }

    result
}
