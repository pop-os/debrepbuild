mod build;
mod download;
mod generate;
mod migrate;
mod pool;
mod prepare;
mod version;

pub use self::migrate::migrate;

use config::Config;
use misc::remove_empty_directories_from;
use rayon;
use rayon::prelude::*;
use std::{env, fs, io};
use std::path::{Path, PathBuf};
use std::process::exit;

pub enum Packages<'a> {
    All,
    Select(&'a [&'a str], bool)
}

pub struct Repo<'a> {
    config: Config,
    packages: Packages<'a>
}

impl<'a> Repo<'a> {
    pub fn prepare(config: Config, packages: Packages<'a>) -> Repo<'a> {
        if let Err(why) = prepare::build_directories(&config.archive) {
            error!("failed to clean build directories: {}", why);
            exit(1);
        }

        if let Err(why) = prepare::create_missing_directories(&config.archive) {
            error!("unable to create directories in current directory: {}", why);
            exit(1);
        }

        Repo { config, packages }
    }

    pub fn clean(self) -> Self {
        if let Err(why) = prepare::package_cleanup(&self.config) {
            error!("failed to clean up file: {}", why);
            exit(1);
        }

        self
    }

    pub fn download(self) -> Self {
        match self.packages {
            Packages::All => download::all(&self.config),
            Packages::Select(ref packages, _) => {
                download::packages(&self.config, packages)
            }
        }

        self
    }

    pub fn build(self) -> Self {
        match self.packages {
            Packages::All => build::all(&self.config),
            Packages::Select(ref packages, force) => {
                build::packages(&self.config, packages, force)
            }
        }

        self
    }

    pub fn generate(self) {
        if let Err(why) = generate_release_files(&self.config) {
            error!("failed to generate dist files: {}", why);
            exit(1);
        }
    }

    pub fn remove(self) -> Self {
        if let Packages::Select(ref packages, _) = self.packages {
            if let Err(why) = prepare::remove(packages, &self.config.archive, &self.config.default_component) {
                error!("failed to remove file: {}", why);
                exit(1);
            }
        }

        self
    }
}

#[derive(Debug, Fail)]
pub enum ReleaseError {
    #[fail(display = "failed to collect component names from {:?}", pool)]
    Components { pool: PathBuf, why: io::Error },
    #[fail(display = "failed to generate distribution files for {}: {}", suite, why)]
    DistGeneration { suite: String, why: io::Error },
    #[fail(display = "failed to generate dist release files for {}: {}", archive, why)]
    DistRelease { archive: String, why: io::Error },
    #[fail(display = "failed to remove dists directory at {:?}: {}", path, why)]
    DistRemoval { path: PathBuf, why: io::Error },
    #[fail(display = "failed to generate InRelease file: {}", why)]
    InRelease { why: io::Error },
    #[fail(display = "pool cleanup failure at {:?}: {}", path, why)]
    PoolCleanup { path: PathBuf, why: io::Error },
    #[fail(display = "failed to generate Release.gpg file: {}", why)]
    ReleaseGPG { why: io::Error },
    #[fail(display = "failed to generate source index: {}", why)]
    Source { why: io::Error },
}

/// Generate the dist release files from the existing binary and source files.
pub fn generate_release_files(sources: &Config) -> Result<(), ReleaseError> {
    env::set_current_dir("repo").expect("unable to switch dir to repo");

    let base = ["dists/", &sources.archive].concat();
    let pool = ["pool/", &sources.archive, "/"].concat();
    let pool_path = &Path::new(&pool);

    {
        let base = &Path::new(&base);
        if base.exists() {
            fs::remove_dir_all(&base)
                .map_err(|why| ReleaseError::DistRemoval { path: base.to_path_buf(), why })?;
        }
    }

    remove_empty_directories_from(pool_path)
        .map_err(|why| ReleaseError::PoolCleanup { path: pool_path.to_path_buf(), why})?;

    let release = PathBuf::from([&base, "/Release"].concat());
    let in_release = PathBuf::from([&base, "/InRelease"].concat());
    let release_gpg = PathBuf::from([&base, "/Release.gpg"].concat());

    let components = collect_components(pool_path, &base).map_err(|why| {
        ReleaseError::Components { pool: pool_path.to_path_buf(), why }
    })?;

    // Generates the dist directory's archives in parallel.
    generate::dists(sources, &base, pool_path, &components)
        .map_err(|why| ReleaseError::DistGeneration {
            suite: sources.archive.clone(),
            why
        })?;

    // TODO: Merge this functionality with generate::dists
    // Then write the source archives in the dist directory
    components.par_iter().map(|component| {
        let pool = [&pool, component.as_str()].concat();
        generate::sources_index(&component, &base, &pool)
            .map_err(|why| ReleaseError::Source { why })
    }).collect::<Result<(), ReleaseError>>()?;

    generate::dists_release(sources, &base, &components)
        .map_err(|why| ReleaseError::DistRelease {
            archive: sources.archive.clone(),
            why,
        })?;

    let (inrelease, release) = rayon::join(
        || {
            generate::gpg_in_release(&sources.email, &release, &in_release)
                .map_err(|why| ReleaseError::InRelease { why })
        },
        || {
            generate::gpg_release(&sources.email, &release, &release_gpg)
                .map_err(|why| ReleaseError::ReleaseGPG { why })
        }
    );

    inrelease.and(release)
}

fn collect_components(pool: &Path, base: &str) -> io::Result<Vec<String>> {
    let mut components = Vec::new();

    for component in pool.read_dir()? {
        if let Ok(component) = component {
            if component.path().is_dir() {
                let component = component.file_name();
                let component = component.to_str().unwrap();
                for arch in &[
                    "binary-amd64",
                    "binary-arm64",
                    "binary-armel",
                    "binary-armhf",
                    "binary-i386",
                    "binary-mips",
                    "binary-mipsel",
                    "binary-mips64el",
                    "binary-ppc64el",
                    "binary-s390x",
                    "binary-all",
                    "source",
                ] {
                    let _ = fs::create_dir_all([&base, "/", component, "/", arch].concat());
                }

                components.push(component.to_owned());
            }
        }
    }

    Ok(components)
}
