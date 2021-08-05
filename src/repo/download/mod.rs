mod direct;
mod repos;
mod request;
mod sources;

use crate::config::Config;
use self::direct::DownloadResult;
use std::io;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;
use reqwest::{self, Client};

pub fn all(config: &Config) {
    let mut errors = Vec::new();

    if let Some(ref ddl_sources) = config.direct {
        for (id, result) in direct::parallel(ddl_sources, &config.archive, &config.default_component)
            .into_iter()
            .enumerate()
        {
            let name = &ddl_sources[id].name;
            match result {
                Ok(DownloadResult::Downloaded(bytes)) => {
                    info!("package '{}' successfully downloaded {} bytes", name, bytes);
                }
                Err(why) => {
                    let msg = format!("package '{}' failed to download: {}", name, why);
                    error!("{}", msg);
                    errors.push(msg);
                }
            }
        }
    }

    if let Some(ref sources) = config.source {
        for (id, result) in sources::parallel(sources, &config.archive)
            .into_iter()
            .enumerate()
        {
            let name = &sources[id].name;
            match result {
                Ok(()) => {
                    info!("package '{}' was successfully fetched", name);
                }
                Err(why) => {
                    let msg = format!("package '{}' failed to download: {}", name, why);
                    error!("{}", msg);
                    errors.push(msg);
                }
            }
        }
    }

    if let Some(ref repos) = config.repos {
        match repos::download(repos, &config.archive, &config.default_component) {
            Ok(()) => {
                info!("all repos fetched successfully");
            }
            Err(why) => {
                let msg = format!("repos failed to fetch: {}", why);
                error!("{}", msg);
                errors.push(msg);
            }
        }

        eprintln!("repos downloaded");
    }

    if ! errors.is_empty() {
        error!("exiting due to error(s): {:#?}", errors);
        exit(1);
    }
}

// TODO: Optimize with a shrinking queue.
pub fn packages(sources: &Config, packages: &[&str]) {
    let mut downloaded = 0;
    let client = Arc::new(Client::new());

    if let Some(ref source) = sources.direct.as_ref() {
        for source in source.iter().filter(|s| packages.contains(&s.name.as_str())) {
            if let Err(why) = direct::download(client.clone(), source, &sources.archive, &sources.default_component) {
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
            if let Err(why) = sources::download(source, &sources.archive) {
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
    #[fail(display = "failed to fetch remote files via dget for {}: {}", url, why)]
    DGet { url: String, why: io::Error },
    #[fail(display = "git exited with an error: {}", why)]
    GitFailed { why: io::Error },
    #[fail(display = "failed to request data for {}: {}", name, why)]
    Request { name: String, why: anyhow::Error }
}
