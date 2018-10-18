use command::Command;
use config::{Source, SourceLocation};
use checksum::hasher;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use reqwest;
use sha2::Sha256;
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use super::DownloadError;

/// Downloads source code repositories in parallel.
pub fn parallel(items: &[Source], suite: &str) -> Vec<Result<(), DownloadError>> {
    // Only up to 8 source clones at a time.
    let thread_pool = ThreadPoolBuilder::new()
        .num_threads(8)
        .build()
        .expect("failed to build thread pool");

    thread_pool.install(move || items.par_iter().map(|i| download(i, suite)).collect())
}

pub fn download(item: &Source, suite: &str) -> Result<(), DownloadError> {
    match item.location {
        Some(SourceLocation::Git { ref git, ref branch }) => {
            match *branch {
                Some(ref _branch) => unimplemented!(),
                None => download_git(git, suite).map_err(|why| DownloadError::GitFailed { why })
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
fn download_git(url: &str, suite: &str) -> io::Result<()> {
    let name: String = {
        url.split_at(url.rfind('/').unwrap() + 1)
            .1
            .replace(".git", "")
    };

    let path = PathBuf::from(["build/", suite, "/", &name].concat());

    if path.exists() {
        info!("pulling {}", name);
        Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(&["pull", "origin", "master"])
            .run()
    } else {
        info!("cloning {}", name);
        let path = ["build/", suite].concat();
        Command::new("git").args(&["-C", &path, "clone", &url]).run()
    }
}
