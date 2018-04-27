extern crate deflate;
extern crate failure;
extern crate rayon;
extern crate reqwest;
extern crate serde;
extern crate toml;
extern crate xz2;

#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate serde_derive;

mod cli;
pub mod config;
pub mod debian;
pub mod download;

use std::{fs, io, path::PathBuf, process::exit};

use cli::Action;
use config::{Config, ConfigFetch};
use download::{DownloadResult, SourceResult};

fn main() {
    match config::parse() {
        Ok(mut sources) => match cli::requested_action() {
            Action::UpdateRepository => update_repository(&sources),
            Action::Fetch(key) => match sources.fetch(&key) {
                Some(value) => println!("{}: {}", key, value),
                None => {
                    eprintln!("config field not found");
                    exit(1);
                }
            },
            Action::FetchConfig => println!("sources.toml: {:#?}", &sources),
            Action::Update(key, value) => match sources.update(&key, value) {
                Ok(()) => match sources.write_to_disk() {
                    Ok(()) => eprintln!("successfully wrote config changes to disk"),
                    Err(why) => {
                        eprintln!("failed to write config changes: {}", why);
                        exit(1);
                    }
                },
                Err(why) => {
                    eprintln!("failed to update {}: {}", key, why);
                    exit(1);
                }
            },
            Action::ConfigHelp => {
                println!("config key[.field] [value]");
                exit(1);
            }
            Action::Unsupported => {
                eprintln!("unsupported command provided");
                exit(1);
            }
        },
        Err(why) => {
            eprintln!("debrepbuild: {}", why);
            exit(1);
        }
    }
}

/// Creates or updates a Debian software repository from a given config
fn update_repository(sources: &Config) {
    let ddl_sources = &sources.direct;
    let mut package_failed = false;
    for (id, result) in download::parallel(ddl_sources).into_iter().enumerate() {
        let name = &ddl_sources[id].name;
        match result {
            Ok(DownloadResult::AlreadyExists) => {
                eprintln!("package '{}' already exists", name);
            }
            Ok(DownloadResult::Downloaded(bytes)) => {
                eprintln!("package '{}' successfully downloaded {} bytes", name, bytes);
            }
            Err(why) => {
                eprintln!("package '{}' failed to download: {}", name, why);
                package_failed = true;
            }
        }
    }

    let branch = &sources.archive;
    if let Some(ref sources) = sources.source {
        let _ = fs::create_dir("sources");
        for (id, result) in download::parallel_sources(sources, branch)
            .into_iter()
            .enumerate()
        {
            let name = &sources[id].name;
            match result {
                Ok(SourceResult::BuildSucceeded) => {
                    eprintln!("package '{}' was successfully fetched & compiled", name);
                }
                Err(why) => {
                    eprintln!("package '{}' failed to build: {}", name, why);
                    package_failed = true;
                }
            }
        }
    }

    if package_failed {
        eprintln!("exiting due to error");
        exit(1);
    }

    if let Err(why) = generate_release_files(&sources) {
        eprintln!("{}", why);
        exit(1);
    }
}

#[derive(Debug, Fail)]
pub enum ReleaseError {
    #[fail(display = "failed to generate release files for binaries: {}", why)]
    Binary { why: io::Error },
    #[fail(display = "failed to generate dist release files for {}: {}", archive, why)]
    Dists { archive: String, why:     io::Error },
    #[fail(display = "failed to generate InRelease file: {}", why)]
    InRelease { why: io::Error },
    #[fail(display = "failed to generate Release.gpg file: {}", why)]
    ReleaseGPG { why: io::Error },
}

/// Generate the dist release files from the existing binary and source files.
fn generate_release_files(sources: &Config) -> Result<(), ReleaseError> {
    let release = PathBuf::from(["dists/", &sources.archive, "/Release"].concat());
    let in_release = PathBuf::from(["dists/", &sources.archive, "/InRelease"].concat());
    let release_gpg = PathBuf::from(["dists/", &sources.archive, "/Release.gpg"].concat());

    debian::generate_binary_files(&sources).map_err(|why| ReleaseError::Binary { why })?;

    debian::generate_dists_release(&sources).map_err(|why| ReleaseError::Dists {
        archive: sources.archive.clone(),
        why,
    })?;

    debian::gpg_in_release(&sources.email, &release, &in_release)
        .map_err(|why| ReleaseError::InRelease { why })?;

    debian::gpg_release(&sources.email, &release, &release_gpg)
        .map_err(|why| ReleaseError::ReleaseGPG { why })
}
