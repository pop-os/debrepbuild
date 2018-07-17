mod download;
mod build;
mod generate;
mod pool;
mod prepare;
mod version;

use std::{env, fs, io};
use std::path::PathBuf;
use std::process::exit;
use config::Config;

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
        if let Err(why) = prepare::create_missing_directories() {
            error!("unable to create directories in current directory: {}", why);
            exit(1);
        }

        if let Err(why) = prepare::package_cleanup(&config) {
            error!("failed to clean up file: {}", why);
            exit(1);
        }

        Repo { config, packages }
    }

    pub fn clean(self) -> Self {
        unimplemented!();
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
        unimplemented!();
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

    generate::generate_binary_files(sources, &base, &pool).map_err(|why| ReleaseError::Binary { why })?;
    generate::generate_sources_index(&base, &pool).map_err(|why| ReleaseError::Source { why })?;
    generate::generate_dists_release(sources, &base).map_err(|why| ReleaseError::Dists {
        archive: sources.archive.clone(),
        why,
    })?;

    generate::gpg_in_release(&sources.email, &release, &in_release)
        .map_err(|why| ReleaseError::InRelease { why })?;

    generate::gpg_release(&sources.email, &release, &release_gpg)
        .map_err(|why| ReleaseError::ReleaseGPG { why })
}
