extern crate ar;
extern crate bus_writer;
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
extern crate rayon;
extern crate reqwest;
extern crate select;
extern crate serde;
extern crate sha1;
extern crate sha2;
extern crate subprocess;
extern crate tar;
extern crate toml;
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
pub mod compress;
pub mod config;
pub mod debian;
pub mod iter_reader;
pub mod misc;
mod repo;

use clap::{Arg, App, AppSettings, SubCommand};
use cli::Action;
use config::ConfigFetch;
use repo::{Packages, Repo};
use std::process::exit;

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

    match config::parse() {
        Ok(mut sources) => {
            debug!("Config: {:#?}", sources);
            match Action::new(&matches) {
                Action::Build(packages, force) => {
                    Repo::prepare(sources, Packages::Select(&packages, force))
                        .download()
                        .build()
                        .generate();
                },
                Action::Clean => {
                    Repo::prepare(sources, Packages::All).clean();
                },
                Action::Dist => {
                    Repo::prepare(sources, Packages::All).generate();
                },
                Action::Fetch(key) => match sources.fetch(&key) {
                    Some(value) => println!("{}: {}", key, value),
                    None => {
                        error!("config field not found");
                        exit(1);
                    }
                },
                Action::FetchConfig => println!("sources.toml: {:#?}", &sources),
                Action::Migrate(packages, from_component, to_component) => {
                    if let Err(why) = repo::migrate(&sources, &packages, from_component, to_component) {
                        error!("migration failed: {}", why);
                        exit(1);
                    }
                },
                Action::Pool => {
                    Repo::prepare(sources, Packages::All).download();
                },
                Action::Remove(packages) => {
                    Repo::prepare(sources, Packages::Select(&packages, false)).remove();
                },
                Action::Update(key, value) => match sources.update(key, value.to_owned()) {
                    Ok(()) => match sources.write_to_disk() {
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
                    Repo::prepare(sources, Packages::All)
                        .download()
                        .build()
                        .generate();
                }
            }
        },
        Err(why) => {
            error!("configuration parsing error: {}", why);
            exit(1);
        }
    }
}
