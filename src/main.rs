extern crate apt_repo_crawler;
extern crate bus_writer;
#[macro_use]
extern crate cascade;
extern crate crossbeam_channel;
extern crate deb_version;
extern crate debarchive;
extern crate deflate;
extern crate digest;
extern crate failure;
extern crate fern;
extern crate glob;
extern crate hex_view;
extern crate itertools;
extern crate libc;
extern crate libflate;
extern crate md5;
extern crate parallel_getter;
extern crate rayon;
extern crate regex;
extern crate reqwest;
extern crate select;
extern crate serde;
extern crate sha1;
extern crate sha2;
extern crate subprocess;
extern crate tempfile;
extern crate toml;
extern crate utime;
extern crate walkdir;
extern crate xz2;

#[macro_use]
extern crate clap;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod cli;
pub mod checksum;
pub mod command;
pub mod compress;
pub mod config;
pub mod debian;
pub mod iter_reader;
pub mod misc;
mod repo;
pub mod url;

use clap::{Arg, App, AppSettings, ArgMatches, SubCommand};
use cli::Action;
use config::{Config, ConfigFetch, SourceLocation};
use repo::{Packages, Repo};
use std::{env, fs, io};
use std::path::PathBuf;
use std::process::exit;
use url::UrlTokenizer;

pub const SHARED_ASSETS: &str = "assets/share/";
pub const PACKAGE_ASSETS: &str = "assets/packages/";

include!(concat!(env!("OUT_DIR"), "/version.rs"));

fn setup_logger() -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        // Exclude logs for crates that we use
        .level(log::LevelFilter::Off)
        // Include only the logs for this binary
        .level_for("debrep", log::LevelFilter::Debug)
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] {}: {}",
                record.level(),
                {
                    let target = record.target();
                    target.find(':').map_or(target, |pos| &target[..pos])
                },
                message
            ))
        })
        .chain(std::io::stderr())
        .apply()?;
    Ok(())
}

fn main() {
    setup_logger().unwrap();
    let version = format!("{} ({})", crate_version!(), short_sha());

    let matches = App::new("Debian Repository Builder")
        .about("Creates and maintains debian repositories")
        .author(crate_authors!())
        .version(version.as_str())
        .global_setting(AppSettings::ColoredHelp)
        .global_setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(Arg::with_name("suites")
            .help("define which suite(s) to operate on [default is all]")
            .long("suites")
            .global(true)
            .value_delimiter(","))
        .subcommand(SubCommand::with_name("build")
            .about("Builds a new repo, or updates an existing one")
            .alias("b")
            .subcommand(SubCommand::with_name("packages")
                .about("builds the specified packages")
                .alias("pkg")
                .arg(Arg::with_name("packages").multiple(true).required(true))
                .arg(Arg::with_name("force")
                    .short("f")
                    .long("force")
                    .group("action")
                    .help("forces the package to be built"))
            )
            .subcommand(SubCommand::with_name("pool")
                .alias("p")
                .about("only builds the pool"))
            .subcommand(SubCommand::with_name("dist")
                .alias("d")
                .about("only builds the dist files"))
        ).subcommand(SubCommand::with_name("clean")
            .about("cleans excess packages from the repository")
        ).subcommand(SubCommand::with_name("config")
            .about("Gets or sets fields within the repo config")
            .alias("c")
            .arg(Arg::with_name("key").required(false))
            .arg(Arg::with_name("value").required(false))
        ).subcommand(SubCommand::with_name("remove")
            .about("removes the specified packages from the repository")
            .alias("r")
            .arg(Arg::with_name("packages").multiple(true).required(true))
        ).subcommand(SubCommand::with_name("update")
            .about("Updates direct download-based packages in the configuration")
            .alias("u")
        ).subcommand(SubCommand::with_name("migrate")
            .about("Moves a package from one component to another, updating both components in the process")
            .alias("m")
            .arg(Arg::with_name("packages")
                .multiple(true)
                .required(true))
            .arg(Arg::with_name("from")
                .help("specifies the component which packages are being moved from")
                .long("from")
                .takes_value(true)
                .required(true))
            .arg(Arg::with_name("to")
                .help("specifies the component which packages are being moved to")
                .long("to")
                .takes_value(true)
                .required(true))
        ).get_matches();

    if let Err(why) = read_configs(&matches) {
        eprintln!("failed to apply configs: {}", why);
        exit(1);
    }
}

fn read_configs(matches: &ArgMatches) -> io::Result<()> {
    let base_directory = env::current_dir()?;
    let mut configs = Vec::new();

    let suites: Vec<PathBuf> = match matches.values_of("suites") {
        Some(suites) => {
            suites
                .map(|x| PathBuf::from(["suites/", &x, ".toml"].concat()))
                .collect()
        }
        None => {
            let mut suites = Vec::new();
            for file in fs::read_dir("suites")? {
                let file = match file {
                    Ok(file) => file,
                    Err(_) => continue
                };

                let filename = file.file_name();
                let filename = match filename.as_os_str().to_str() {
                    Some(filename) => filename,
                    None => continue
                };

                if filename.ends_with(".toml") {
                    suites.push(file.path());
                }
            }

            suites
        }
    };

    for suite in suites {
        let mut config = config::parse(suite).map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("configuration parsing error: {}", why)
        ))?;

        if let Some(ref mut sources) = config.source {
            for source in sources {
                if let Some(ref version) = source.version {
                    if let Some(SourceLocation::Dsc { ref mut dsc }) = source.location {
                        *dsc = UrlTokenizer::finalize(&dsc, &source.name, &version)
                            .map_err(|text|
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("unsupported variable: {}", text)
                                )
                            )?;
                    }
                }
            }
        }

        configs.push(config);
    }

    for config in configs {
        apply_config(config, matches);
        env::set_current_dir(&base_directory)?;
    }

    Ok(())
}

fn apply_config(mut config: Config, matches: &ArgMatches) {
    info!("Building from config at {}", config.path.display());
    match Action::new(&matches) {
        Action::Build(packages, force) => {
            Repo::prepare(config, Packages::Select(&packages, force))
                .download()
                .build()
                .generate();
        },
        Action::Clean => {
            Repo::prepare(config, Packages::All).clean();
        },
        Action::Dist => {
            Repo::prepare(config, Packages::All).generate();
        },
        Action::Fetch(key) => match config.fetch(&key) {
            Some(value) => println!("{}: {}", key, value),
            None => {
                error!("config field not found");
                exit(1);
            }
        },
        Action::FetchConfig => println!("{}: {:#?}", config.path.display(), &config),
        Action::Migrate(packages, from_component, to_component) => {
            if let Err(why) = repo::migrate(&config, &packages, from_component, to_component) {
                error!("migration failed: {}", why);
                exit(1);
            }
        },
        Action::Pool => {
            Repo::prepare(config, Packages::All).download();
        },
        Action::Remove(packages) => {
            Repo::prepare(config, Packages::Select(&packages, false)).remove();
        },
        Action::Update(key, value) => match config.update(key, value.to_owned()) {
            Ok(()) => match config.write_to_disk() {
                Ok(()) => info!("successfully wrote config changes to disk"),
                Err(why) => {
                    error!("failed to write config changes: {}", why);
                    exit(1);
                }
            },
            Err(why) => {
                error!("failed to update {}: {}", key, why);
                exit(1);
            }
        },
        Action::UpdateRepository => {
            Repo::prepare(config, Packages::All)
                .download()
                .build()
                .generate();
        }
    }
}
