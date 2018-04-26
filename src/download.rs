use std::{
    env,
    fs::{create_dir_all, File},
    io,
    path::{Path, PathBuf},
    process::Command,
};

use rayon::prelude::*;
use reqwest::{self, header::ContentLength, Client, Response};

use config::{Direct, PackageEntry, Source};

#[derive(Debug, Fail)]
pub enum DownloadError {
    #[fail(display = "build command failed: {}", why)]
    BuildCommand { why: io::Error },
    #[fail(display = "failed to build from source")]
    BuildFailed,
    #[fail(display = "unable to download '{}': {}", item, why)]
    Request { item: String, why:  reqwest::Error },
    #[fail(display = "git command failed")]
    GitFailed,
    #[fail(display = "unable to git '{}': {}", item, why)]
    GitRequest { item: String, why:  io::Error },
    #[fail(display = "unable to open '{}': {}", item, why)]
    File { item: String, why:  io::Error },
    #[fail(display = "unsupported cvs for source: {}", cvs)]
    UnsupportedCVS { cvs: String },
}

pub enum DownloadResult {
    Downloaded(u64),
    AlreadyExists,
    BuildSucceeded,
}

fn download<P: PackageEntry>(client: &Client, item: &P) -> Result<DownloadResult, DownloadError> {
    eprintln!(" - {}", item.get_name());

    let parent = item.destination();
    let filename = item.file_name();
    let destination = parent.join(filename);

    let dest_result = if destination.exists() {
        let mut capacity = File::open(&destination)
            .and_then(|file| file.metadata().map(|x| x.len()))
            .unwrap_or(0);

        let response = client
            .head(item.get_url())
            .send()
            .map_err(|why| DownloadError::Request {
                item: item.get_name().to_owned(),
                why,
            })?;

        if check_length(&response, capacity) {
            return Ok(DownloadResult::AlreadyExists);
        }

        File::create(destination)
    } else {
        create_dir_all(&parent).and_then(|_| File::create(destination))
    };

    let mut dest = dest_result.map_err(|why| DownloadError::File {
        item: item.get_name().to_owned(),
        why,
    })?;

    let mut response = client
        .get(item.get_url())
        .send()
        .map_err(|why| DownloadError::Request {
            item: item.get_name().to_owned(),
            why,
        })?;

    response
        .copy_to(&mut dest)
        .map(|x| DownloadResult::Downloaded(x))
        .map_err(|why| DownloadError::Request {
            item: item.get_name().to_owned(),
            why,
        })
}

fn check_length(response: &Response, compared: u64) -> bool {
    response
        .headers()
        .get::<ContentLength>()
        .map(|len| **len)
        .unwrap_or(0) == compared
}

fn build(item: &Source, path: &Path, branch: &str) -> Result<DownloadResult, DownloadError> {
    let _ = env::set_current_dir(path);
    if let Some(ref prebuild) = item.prebuild {
        for command in prebuild {
            let exit_status = Command::new("sh")
                .args(&["-c", command])
                .status()
                .map_err(|why| DownloadError::BuildCommand { why })?;

            if !exit_status.success() {
                return Err(DownloadError::BuildFailed);
            }
        }
    }

    let exit_status = Command::new("sbuild")
        .arg("--arch-all")
        .arg(format!("--dist={}", branch))
        .arg("--quiet")
        .arg(".")
        .status()
        .map_err(|why| DownloadError::BuildCommand { why })?;

    if exit_status.success() {
        Ok(DownloadResult::BuildSucceeded)
    } else {
        Err(DownloadError::BuildFailed)
    }
}

fn download_git(item: &Source, branch: &str) -> Result<DownloadResult, DownloadError> {
    let path = PathBuf::from(["sources/", item.get_name()].concat());

    if path.exists() {
        let exit_status = Command::new("git")
            .args(&["-C", "sources", "pull", "origin", "master"])
            .status()
            .map_err(|why| DownloadError::GitRequest {
                item: item.get_name().to_owned(),
                why,
            })?;

        if !exit_status.success() {
            return Err(DownloadError::GitFailed);
        }
    } else {
        let exit_status = Command::new("git")
            .args(&["-C", "sources", "clone", item.get_url()])
            .status()
            .map_err(|why| DownloadError::GitRequest {
                item: item.get_name().to_owned(),
                why,
            })?;

        if !exit_status.success() {
            return Err(DownloadError::GitFailed);
        }
    }

    build(item, &path, branch)
}

pub fn parallel(items: &[Direct]) -> Vec<Result<DownloadResult, DownloadError>> {
    eprintln!("downloading packages in parallel");
    let client = Client::new();
    items
        .par_iter()
        .map(|item| download(&client, item))
        .collect()
}

pub fn parallel_sources(
    items: &[Source],
    branch: &str,
) -> Vec<Result<DownloadResult, DownloadError>> {
    eprintln!("downloading sources in parallel");
    items
        .par_iter()
        .map(|item| match item.cvs.as_str() {
            "git" => download_git(item, branch),
            _ => Err(DownloadError::UnsupportedCVS {
                cvs: item.cvs.clone(),
            }),
        })
        .collect()
}
