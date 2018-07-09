extern crate deflate;
extern crate failure;
extern crate glob;
extern crate libc;
extern crate md5;
extern crate rayon;
extern crate reqwest;
extern crate select;
extern crate serde;
extern crate toml;
extern crate walkdir;
extern crate xz2;

#[macro_use]
extern crate clap;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate serde_derive;

mod cli;
pub mod config;
pub mod debian;
mod direct;
pub mod misc;
mod sources;
mod update;

use clap::{Arg, App, SubCommand};
use cli::Action;
use config::{Config, ConfigFetch};
use direct::download::DownloadResult;
use std::{env, fs, io};
use std::path::{Path, PathBuf};
use std::process::exit;

use update::update_packages;
use reqwest::Client;
use sources::build::build;

fn main() {
    let mut app = App::new("Debian Repository Builder")
        .about("Creates and maintains debian repositories")
        .author(crate_authors!())
        .version(crate_version!())
        .subcommand(
            SubCommand::with_name("build")
                .about("Builds a new repo, or updates an existing one")
                .arg(Arg::with_name("package").required(false))
        ).subcommand(
            SubCommand::with_name("config")
                .about("Gets or sets fields within the repo config")
                .arg(Arg::with_name("key").required(false))
                .arg(Arg::with_name("value").required(false))
        ).subcommand(
            SubCommand::with_name("update")
                .about("Updates direct download-based packages in the configuration")
        );

    let matches = app.clone().get_matches();

    match config::parse() {
        Ok(mut sources) => match cli::requested_action(&matches) {
            Action::Build(package) => update_package(&sources, package),
            Action::UpdateRepository => update_repository(&sources),
            Action::Fetch(key) => match sources.fetch(&key) {
                Some(value) => println!("{}: {}", key, value),
                None => {
                    eprintln!("config field not found");
                    exit(1);
                }
            },
            Action::FetchConfig => println!("sources.toml: {:#?}", &sources),
            Action::Update(key, value) => match sources.update(key, value.to_owned()) {
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
            Action::UpdatePackages => if let Err(why) = update_packages(&mut sources) {
                eprintln!("failed to update config: {}", why);
                exit(1);
            },
            Action::ConfigHelp => {
                let _ = app.print_help();
                exit(1);
            }
        },
        Err(why) => {
            eprintln!("debrep: {}", why);
            exit(1);
        }
    }
}

pub const SHARED_ASSETS: &str = "assets/share/";
pub const PACKAGE_ASSETS: &str = "assets/packages/";

fn update_package(sources: &Config, package: &str) {
    if let Some(ref source) = sources.direct.as_ref().and_then(|ddl| ddl.iter().find(|x| x.name == package)) {
        if let Err(why) = direct::download::download(&Client::new(), source, &sources.archive) {
            eprintln!("debrep: failed to download {}: {}", package, why);
            exit(1);
        }
        return;
    }

    if let Some(ref source) = sources.source.as_ref().and_then(|s| s.iter().find(|x| x.name == package)) {
        if let Err(why) = sources::download::download(source) {
            eprintln!("debrep: failed to download source {}: {}", package, why);
            exit(1);
        }

        let pwd = env::current_dir().unwrap();

        if let Err(why) = build(source, &pwd, &sources.archive) {
            eprintln!("debrep: package '{}' failed to build: {}", source.name, why);
            exit(1);
        }
    }

    let include_dir = Path::new("include");
    if include_dir.is_dir() {
        eprintln!("copying packages from {}", include_dir.display());
        if let Err(why) = misc::cp_to_pool("include", &sources.archive) {
            eprintln!("failed to copy packages from include directory: {}", why);
            exit(1);
        }
    }

    if let Err(why) = generate_release_files(sources) {
        eprintln!("{}", why);
        exit(1);
    }
}

/// Creates or updates a Debian software repository from a given config
fn update_repository(sources: &Config) {
    let dirs_result = [SHARED_ASSETS, PACKAGE_ASSETS, "build", "record", "sources"].iter()
        .map(|dir| if Path::new(dir).exists() { Ok(()) } else { fs::create_dir_all(dir) })
        .collect::<io::Result<()>>();

    if let Err(why) = dirs_result {
        eprintln!("unable to create directories in current directory: {}", why);
        exit(1);
    }

    let branch = &sources.archive;
    let mut package_failed = false;
    if let Some(ref ddl_sources) = sources.direct {
        for (id, result) in direct::download::parallel(ddl_sources, branch)
            .into_iter()
            .enumerate()
        {
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
    }

    let pwd = env::current_dir().unwrap();
    if let Some(ref sources) = sources.source {
        for (id, result) in sources::download::parallel(sources)
            .into_iter()
            .enumerate()
        {
            let name = &sources[id].name;
            match result {
                Ok(()) => {
                    eprintln!("package '{}' was successfully fetched", name);
                }
                Err(why) => {
                    eprintln!("package '{}' failed to download: {}", name, why);
                    package_failed = true;
                }
            }
        }

        for source in sources {
            if let Err(why) = build(source, &pwd, branch) {
                eprintln!("package '{}' failed to build: {}", source.name, why);
                package_failed = true;
                break
            }
        }
    }

    if package_failed {
        eprintln!("exiting due to error");
        exit(1);
    }

    let include_dir = Path::new("include");
    if include_dir.is_dir() {
        eprintln!("copying packages from {}", include_dir.display());
        if let Err(why) = misc::cp_to_pool("include", branch) {
            eprintln!("failed to copy packages from include directory: {}", why);
            exit(1);
        }
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
    #[fail(display = "failed to generate source index: {}", why)]
    Source { why: io::Error },
    #[fail(display = "failed to generate dist release files for {}: {}", archive, why)]
    Dists { archive: String, why: io::Error },
    #[fail(display = "failed to generate InRelease file: {}", why)]
    InRelease { why: io::Error },
    #[fail(display = "failed to generate Release.gpg file: {}", why)]
    ReleaseGPG { why: io::Error },
}

/// Generate the dist release files from the existing binary and source files.
fn generate_release_files(sources: &Config) -> Result<(), ReleaseError> {
    env::set_current_dir("repo").expect("unable to switch dir to repo");
    let base = ["dists/", &sources.archive].concat();
    let pool = ["pool/", &sources.archive, "/main"].concat();
    let _ = fs::create_dir_all(&base);

    let release = PathBuf::from([&base, "/Release"].concat());
    let in_release = PathBuf::from([&base, "/InRelease"].concat());
    let release_gpg = PathBuf::from([&base, "/Release.gpg"].concat());

    debian::generate_binary_files(sources, &base, &pool).map_err(|why| ReleaseError::Binary { why })?;
    debian::generate_sources_index(&base, &pool).map_err(|why| ReleaseError::Source { why })?;
    debian::generate_dists_release(sources, &base).map_err(|why| ReleaseError::Dists {
        archive: sources.archive.clone(),
        why,
    })?;

    debian::gpg_in_release(&sources.email, &release, &in_release)
        .map_err(|why| ReleaseError::InRelease { why })?;

    debian::gpg_release(&sources.email, &release, &release_gpg)
        .map_err(|why| ReleaseError::ReleaseGPG { why })
}
