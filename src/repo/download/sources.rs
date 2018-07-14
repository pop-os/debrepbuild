use config::{Source, SourceLocation};
use super::checksum::sha2_256_digest;
use rayon::prelude::*;
use reqwest;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;
use super::DownloadError;

/// Downloads source code repositories in parallel.
pub fn parallel(items: &[Source]) -> Vec<Result<(), DownloadError>> {
    items.par_iter().map(download).collect()
}

pub fn download(item: &Source) -> Result<(), DownloadError> {
    match item.location {
        Some(SourceLocation::Git { ref url, ref branch }) => {
            match *branch {
                Some(ref _branch) => unimplemented!(),
                None => download_git(url)
            }
        },
        Some(SourceLocation::URL { ref url, ref checksum }) => {
            download_(item, url, checksum)
        },
        None => Ok(())
    }
}

fn download_(item: &Source, url: &str, checksum: &str) -> Result<(), DownloadError> {
    let filename = &url[url.rfind('/').map_or(0, |x| x + 1)..];
    let destination = PathBuf::from(["assets/cache/", &item.name, "_", &filename].concat());

    let requires_download = if destination.is_file() {
        let digest = File::open(&destination)
            .and_then(sha2_256_digest)
            .map_err(|why| DownloadError::Open {
                file: destination.clone(),
                why
            })?;

        &digest != checksum
    } else {
        true
    };

    if requires_download {
        warn!("checksum did not match for {}. downloading from {}", &item.name, url);
        let mut file = File::create(&destination).map_err(|why| DownloadError::Open {
            file: destination.clone(),
            why
        })?;

        reqwest::get(url)
            .and_then(|mut request| request.copy_to(&mut file))
            .map_err(|why| DownloadError::Request { name: filename.to_owned(), why })?;
    }

    let digest = File::open(&destination)
        .and_then(sha2_256_digest)
        .map_err(|why| DownloadError::Open {
            file: destination.clone(),
            why
        })?;

    if &digest == checksum {
        Ok(())
    } else {
        Err(DownloadError::ChecksumInvalid {
            name: item.name.clone(),
            expected: checksum.to_owned(),
            received: digest
        })
    }
}

/// Downloads the source repository via git, then attempts to build it.
fn download_git(url: &str) -> Result<(), DownloadError> {
    let name: String = {
        url.split_at(url.rfind('/').unwrap() + 1)
            .1
            .replace(".git", "")
    };

    let path = PathBuf::from(["build/", &name].concat());

    if path.exists() {
        info!("pulling {}", name);
        let exit_status = Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(&["pull", "origin", "master"])
            .status()
            .map_err(|why| DownloadError::CommandFailed {
                cmd: "git",
                why,
            })?;

        if !exit_status.success() {
            return Err(DownloadError::GitFailed { name: name.to_owned() });
        }
    } else {
        info!("cloning {}", name);
        let exit_status = Command::new("git")
            .args(&["-C", "build", "clone", &url])
            .status()
            .map_err(|why| DownloadError::CommandFailed {
                cmd: "git",
                why,
            })?;

        if !exit_status.success() {
            return Err(DownloadError::GitFailed { name: name.to_owned() });
        }
    }

    Ok(())
}
