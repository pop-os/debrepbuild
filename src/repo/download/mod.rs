mod direct;
mod request;
mod sources;
mod url;

use config::Config;
use self::direct::DownloadResult;
use std::io;
use std::path::PathBuf;
use std::process::exit;
use reqwest::{self, Client};

pub fn all(config: &Config) {
    let mut package_failed = false;
    if let Some(ref ddl_sources) = config.direct {
        for (id, result) in direct::parallel(ddl_sources, &config.archive, &config.default_branch)
            .into_iter()
            .enumerate()
        {
            let name = &ddl_sources[id].name;
            match result {
                Ok(DownloadResult::Downloaded(bytes)) => {
                    info!("package '{}' successfully downloaded {} bytes", name, bytes);
                }
                Err(why) => {
                    error!("package '{}' failed to download: {}", name, why);
                    package_failed = true;
                }
            }
        }
    }

    if let Some(ref sources) = config.source {
        for (id, result) in sources::parallel(sources)
            .into_iter()
            .enumerate()
        {
            let name = &sources[id].name;
            match result {
                Ok(()) => {
                    info!("package '{}' was successfully fetched", name);
                }
                Err(why) => {
                    error!("package '{}' failed to download: {}", name, why);
                    package_failed = true;
                }
            }
        }
    }

    if package_failed {
        error!("exiting due to error");
        exit(1);
    }
}

// TODO: Optimize with a shrinking queue.
pub fn packages(sources: &Config, packages: &[&str]) {
    let mut downloaded = 0;

    if let Some(ref source) = sources.direct.as_ref() {
        for source in source.iter().filter(|s| packages.contains(&s.name.as_str())) {
            if let Err(why) = direct::download(&Client::new(), source, &sources.archive, &sources.default_branch) {
                error!("failed to download {}: {}", &source.name, why);
                exit(1);
            }

            downloaded += 1;
            if downloaded == packages.len() {
                return;
            }
        }
    }

    if let Some(ref source) = sources.source.as_ref() {
        for source in source.iter().filter(|s| packages.contains(&s.name.as_str())) {
            if let Err(why) = sources::download(source) {
                error!("failed to download source {}: {}", &source.name, why);
                exit(1);
            }

            downloaded += 1;
            if downloaded == packages.len() {
                return;
            }
        }
    }
}

#[derive(Debug, Fail)]
pub enum DownloadError {
    #[fail(display = "failed to open file at {:?}: {}", file, why)]
    Open { file: PathBuf, why: io::Error },
    #[fail(display = "checksum for {} is invalid -- expected {}, but received {}", name, expected, received)]
    ChecksumInvalid { name: String, expected: String, received: String },
    #[fail(display = "{} command failed to execute: {}", cmd, why)]
    CommandFailed { cmd: &'static str, why: io::Error },
    #[fail(display = "git exited with an error on job {}", name)]
    GitFailed { name: String },
    #[fail(display = "failed to request data for {}: {}", name, why)]
    Request { name: String, why: reqwest::Error }
}
