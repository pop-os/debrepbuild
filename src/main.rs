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
pub mod debian;
pub mod download;
pub mod sources;

use std::{path::PathBuf, process::exit, fs};

use cli::Action;
use download::DownloadResult;
use sources::{Config, ConfigFetch};

fn main() {
    match sources::parse() {
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
            Ok(DownloadResult::BuildSucceeded) => {
                eprintln!("package '{}' was successfully fetched & compiled", name);
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
        for (id, result) in download::parallel_sources(sources, branch).into_iter().enumerate() {
            let name = &sources[id].name;
            match result {
                Ok(DownloadResult::AlreadyExists) => {
                    eprintln!("package '{}' already exists", name);
                }
                Ok(DownloadResult::Downloaded(bytes)) => {
                    eprintln!("package '{}' successfully downloaded {} bytes", name, bytes);
                }
                Ok(DownloadResult::BuildSucceeded) => {
                    eprintln!("package '{}' was successfully fetched & compiled", name);
                }
                Err(why) => {
                    eprintln!("package '{}' failed to download: {}", name, why);
                    package_failed = true;
                }
            }
        }
    }

    if package_failed {
        eprintln!("exiting due to error");
        exit(1);
    }

    if let Err(why) = debian::generate_binary_files(&sources, "amd64") {
        eprintln!("failed to generate files for binaries: {}", why);
        exit(1);
    }

    let release_path = PathBuf::from(["dists/", &sources.archive, "/Release"].concat());
    let in_release_path = PathBuf::from(["dists/", &sources.archive, "/InRelease"].concat());
    let release_gpg_path = PathBuf::from(["dists/", &sources.archive, "/Release.gpg"].concat());

    if let Err(why) = debian::generate_dists_release(&sources) {
        eprintln!("failed to generate release file for dists: {}", why);
        exit(1);
    }

    if let Err(why) = debian::gpg_in_release(&sources.email, &release_path, &in_release_path) {
        eprintln!("failed to generate InRelease file: {}", why);
        exit(1);
    }

    if let Err(why) = debian::gpg_release(&sources.email, &release_path, &release_gpg_path) {
        eprintln!("failed to generate Release.gpg file: {}", why);
        exit(1);
    }
}
