use std::{
    fs::{create_dir_all, File},
    io,
    process::Command,
};

use rayon::prelude::*;
use reqwest::{self, header::ContentLength, Client, Response};

use sources::{Direct, PackageEntry, Source};

#[derive(Debug, Fail)]
pub enum DownloadError {
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
    GitSucceeded,
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

fn download_git(item: &str, url: &str) -> Result<DownloadResult, DownloadError> {
    let exit_status = Command::new("git")
        .args(&["clone", url])
        .status()
        .map_err(|why| DownloadError::GitRequest {
            item: item.to_owned(),
            why,
        })?;

    if exit_status.success() {
        Ok(DownloadResult::GitSucceeded)
    } else {
        Err(DownloadError::GitFailed)
    }
}

pub fn parallel(items: &[Direct]) -> Vec<Result<DownloadResult, DownloadError>> {
    eprintln!("downloading packages in parallel");
    let client = Client::new();
    items
        .par_iter()
        .map(|item| download(&client, item))
        .collect()
}

pub fn parallel_sources(items: &[Source]) -> Vec<Result<DownloadResult, DownloadError>> {
    eprintln!("downloading sources in parallel");
    items
        .par_iter()
        .map(|item| match item.cvs.as_str() {
            "git" => download_git(item.get_name(), item.get_url()),
            _ => Err(DownloadError::UnsupportedCVS {
                cvs: item.cvs.clone(),
            }),
        })
        .collect()
}
