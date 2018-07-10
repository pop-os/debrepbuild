use super::SourceError;
use config::{Source, SourceLocation};
use misc::{extract, md5_digest};
use rayon::prelude::*;
use reqwest;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;

/// Downloads source code repositories in parallel.
pub fn parallel(items: &[Source]) -> Vec<Result<(), SourceError>> {
    items.par_iter().map(download).collect()
}

pub fn download(item: &Source) -> Result<(), SourceError> {
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

fn download_(item: &Source, url: &str, checksum: &str) -> Result<(), SourceError> {
    let filename = &url[url.rfind('/').map_or(0, |x| x + 1)..];
    let destination = PathBuf::from(["assets/cache/", &item.name, "_", &filename].concat());

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
        warn!("checksum did not match for {}. downloading from {}", &item.name, url);
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
        // match item.build_on.as_ref().map(|x| x.as_str()) {
        //     Some("changelog") => match item.debian {
        //         Some(DebianPath::URL { ref url, ref checksum }) => unimplemented!(),
        //         Some(DebianPath::Branch { ref branch }) => unimplemented!(),
        //         None => {
        //             let changelog_path = PathBuf::from(["debian/", &item.name, "/changelog"].concat()));
        //             if let Ok(changelog) => version::changelog(&changelog_path) {
        //
        //             }
        //         }
        //     }
        //     _ => ()
        // }
        let path = PathBuf::from(["build/", &item.name].concat());
        extract(&destination, &path)
            .map_err(|why| SourceError::TarExtract { path: destination, why })
    } else {
        Err(SourceError::InvalidChecksum {
            expected: checksum.to_owned(),
            received: digest
        })
    }
}

/// Downloads the source repository via git, then attempts to build it.
fn download_git(url: &str) -> Result<(), SourceError> {
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
            .map_err(|why| SourceError::GitRequest {
                item: name.to_owned(),
                why,
            })?;

        if !exit_status.success() {
            return Err(SourceError::GitFailed);
        }
    } else {
        info!("cloning {}", name);
        let exit_status = Command::new("git")
            .args(&["-C", "build", "clone", &url])
            .status()
            .map_err(|why| SourceError::GitRequest {
                item: name.to_owned(),
                why,
            })?;

        if !exit_status.success() {
            return Err(SourceError::GitFailed);
        }
    }

    Ok(())
}
