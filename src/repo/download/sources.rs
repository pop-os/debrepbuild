use command::Command;
use config::{Source, SourceLocation};
use checksum::hasher;
use rayon::prelude::*;
use reqwest;
use sha2::Sha256;
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use super::DownloadError;

/// Downloads source code repositories in parallel.
pub fn parallel(items: &[Source]) -> Vec<Result<(), DownloadError>> {
    items.par_iter().map(download).collect()
}

pub fn download(item: &Source) -> Result<(), DownloadError> {
    match item.location {
        Some(SourceLocation::Git { ref git, ref branch }) => {
            match *branch {
                Some(ref _branch) => unimplemented!(),
                None => download_git(git).map_err(|why| DownloadError::GitFailed { why })
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
            .and_then(hasher::<Sha256, File>)
            .map_err(|why| DownloadError::Open {
                file: destination.clone(),
                why
            })?;

        digest != checksum
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
        .and_then(hasher::<Sha256, File>)
        .map_err(|why| DownloadError::Open {
            file: destination.clone(),
            why
        })?;

    if digest == checksum {
        Ok(())
    } else {
        let _ = fs::remove_file(&destination);
        Err(DownloadError::ChecksumInvalid {
            name: item.name.clone(),
            expected: checksum.to_owned(),
            received: digest
        })
    }
}

/// Downloads the source repository via git, then attempts to build it.
fn download_git(url: &str) -> io::Result<()> {
    let name: String = {
        url.split_at(url.rfind('/').unwrap() + 1)
            .1
            .replace(".git", "")
    };

    let path = PathBuf::from(["build/", &name].concat());

    if path.exists() {
        info!("pulling {}", name);
        Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(&["pull", "origin", "master"])
            .run()
    } else {
        info!("cloning {}", name);
        Command::new("git").args(&["-C", "build", "clone", &url]).run()
    }
}
