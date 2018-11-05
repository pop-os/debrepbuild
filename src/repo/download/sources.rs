use command::Command;
use config::{Source, SourceLocation};
use checksum::hasher;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use reqwest;
use sha2::Sha256;
use std::fs::{self, File};
use std::{env, io};
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
        Some(SourceLocation::Git { ref git, ref branch, ref commit }) => {
            download_git(&item.name, git, suite, branch, commit).map_err(|why| DownloadError::GitFailed { why })
        },
        Some(SourceLocation::URL { ref url, ref checksum }) => {
            download_(item, url, checksum)
        },
        Some(SourceLocation::Dsc { ref dsc }) => {
            download_dsc(item, dsc, suite).map_err(|why| {
                DownloadError::DGet { url: dsc.to_owned(), why }
            })
        }
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
///
/// - If the build directory does not exist, it will be cloned.
/// - Otherwise, the sources will be pulled from the build directory.
fn download_git(name: &str, url: &str, suite: &str, branch: &Option<String>, commit: &Option<String>) -> io::Result<()> {
    let path = env::current_dir()
        .expect("failed to get current directory")
        .join(["build/", suite].concat());

    let path_with_name = path.join(name);

    let clone = || -> io::Result<()> {
        Command::new("git").arg("-C").arg(&path).args(&["clone", &url, name]).run()
    };

    let pull = |branch: &str| -> io::Result<()> {
        Command::new("git")
            .arg("-C")
            .arg(&path_with_name)
            .args(&["pull", "origin", branch])
            .run()
    };

    let reset = || -> io::Result<()> {
        if let Some(commit) = commit {
            Command::new("git")
                .arg("-C")
                .arg(&path_with_name)
                .args(&["reset", "--hard", &commit])
                .run()?;
        }

        Ok(())
    };

    let checkout = || -> io::Result<&str> {
        match branch {
            Some(branch) => {
                Command::new("git")
                    .arg("-C")
                    .arg(&path_with_name)
                    .args(&["checkout", &branch])
                    .run()?;
                Ok(branch.as_str())
            }
            None => Ok("master")
        }
    };

    if path_with_name.exists() {
        let branch = checkout()?;

        if let Some(commit) = commit {
            let current_revision = Command::new("git")
                .arg("-C")
                .arg(&path_with_name)
                .args(&["rev-parse", branch])
                .run_with_stdout()?;

            if current_revision.starts_with(commit.as_str()) {
                return Ok(());
            }
        }

        pull(branch)?;
        reset()?;
    } else {
        clone()?;
        checkout()?;
        reset()?;
    }

    Ok(())
}

/// Downloads a debian package's sources from the given remote `dsc` URL.
///
/// - The files will only be downloaded, not extracted.
/// - The files will only be downloaded if they do not already exist.
fn download_dsc(item: &Source, dsc: &str, suite: &str) -> io::Result<()> {
    let path = PathBuf::from(["build/", suite, "/", &item.name].concat());
    let mut result = Ok(());
    if ! path.join(::misc::filename_from_url(dsc)).exists() {
        fs::create_dir_all(&path)?;
        let cwd = env::current_dir()?;
        env::set_current_dir(&path)?;

        result = Command::new("dget").args(&["-uxqd", dsc]).run();
        if let Err(why) = env::set_current_dir(cwd) {
            panic!("failed to set directory to original location: {}", why);
        }
    }

    result
}
