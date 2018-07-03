use rayon::prelude::*;
use reqwest::{self, header::ContentLength, Client, Response};
use std::{
    fs::{create_dir_all, File},
    io,
};

use config::{Direct, PackageEntry};

/// Possible errors that may happen when attempting to download Debian packages and source code.
#[derive(Debug, Fail)]
pub enum DownloadError {
    #[fail(display = "unable to download '{}': {}", item, why)]
    Request { item: String, why:  reqwest::Error },
    #[fail(display = "unable to open '{}': {}", item, why)]
    File { item: String, why:  io::Error },
}

/// Possible messages that may be returned when a download has succeeded.
pub enum DownloadResult {
    Downloaded(u64),
    AlreadyExists,
}

/// Given an item with a URL, download the item if the item does not already exist.
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
        .map(DownloadResult::Downloaded)
        .map_err(|why| DownloadError::Request {
            item: item.get_name().to_owned(),
            why,
        })
}

/// Compares the length reported by the requested header to the length of existing file.
fn check_length(response: &Response, compared: u64) -> bool {
    response
        .headers()
        .get::<ContentLength>()
        .map(|len| **len)
        .unwrap_or(0) == compared
}

/// Downloads pre-built Debian packages in parallel
pub fn parallel(items: &[Direct]) -> Vec<Result<DownloadResult, DownloadError>> {
    eprintln!("downloading packages in parallel");
    let client = Client::new();
    items
        .par_iter()
        .map(|item| download(&client, item))
        .collect()
}
