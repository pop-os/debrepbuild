use super::build::build;
use super::SourceError;
use config::{Source, SourceLocation};
use misc::{extract_tar, md5_digest};
use rayon::prelude::*;
use reqwest;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;

/// Downloads source code repositories and builds them in parallel.
pub fn parallel(items: &[Source], branch: &str) -> Vec<Result<(), SourceError>> {
    eprintln!("downloading sources in parallel");
    items
        .par_iter()
        .map(|item| match item.location {
            SourceLocation::Path { ref path } => build(item, path, branch),
            SourceLocation::Git { ref url } => download_git(item, url, branch),
            SourceLocation::URL { ref url, ref checksum } => {
                download(item, url, checksum, branch)
            },
        })
        .collect()
}

fn download(item: &Source, url: &str, checksum: &str, branch: &str) -> Result<(), SourceError> {
    let filename = &url[url.rfind('/').map_or(0, |x| x + 1)..];
    let destination = PathBuf::from(["assets/artifacts/", &item.name, "_", &filename].concat());

    let requires_download = if destination.is_file() {
        let digest = File::open(&destination)
            .and_then(md5_digest)
            .map_err(|why| SourceError::File {
                file: destination.clone(),
                why
            })?;

        &digest != checksum
    } else {
        true
    };

    if requires_download {
        eprintln!("checksum did not match for {}. downloading from {}", &item.name, url);
        let mut file = File::create(&destination).map_err(|why| SourceError::File {
            file: destination.clone(),
            why
        })?;

        reqwest::get(url)
            .and_then(|mut request| request.copy_to(&mut file))
            .map_err(|why| SourceError::Request { item: filename.to_owned(), why })?;
    }

    let digest = File::open(&destination)
        .and_then(md5_digest)
        .map_err(|why| SourceError::File {
            file: destination.clone(),
            why
        })?;

    if &digest == checksum {
        let path = PathBuf::from(["sources/", &item.name].concat());
        extract_tar(&destination, &path)
            .map_err(|why| SourceError::TarExtract { path: destination, why })
            .and_then(|_| build(item, &path, branch))
    } else {
        Err(SourceError::InvalidChecksum {
            expected: checksum.to_owned(),
            received: digest
        })
    }
}

/// Downloads the source repository via git, then attempts to build it.
fn download_git(item: &Source, url: &str, branch: &str) -> Result<(), SourceError> {
    let name: String = {
        url.split_at(url.rfind('/').unwrap() + 1)
            .1
            .replace(".git", "")
    };

    let path = PathBuf::from(["sources/", &name].concat());

    if path.exists() {
        eprintln!("pulling {}", name);
        let exit_status = Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(&["pull", "origin", "master"])
            .status()
            .map_err(|why| SourceError::GitRequest {
                item: name.to_owned(),
                why,
            })?;

        if !exit_status.success() {
            return Err(SourceError::GitFailed);
        }
    } else {
        eprintln!("cloning {}", name);
        let exit_status = Command::new("git")
            .args(&["-C", "sources", "clone", &url])
            .status()
            .map_err(|why| SourceError::GitRequest {
                item: name.to_owned(),
                why,
            })?;

        if !exit_status.success() {
            return Err(SourceError::GitFailed);
        }
    }

    build(item, &path, branch)
}
