use rayon::prelude::*;
use reqwest::{self, Client, Response};
use reqwest::header::ContentLength;
use std::fs::{create_dir_all, File};
use std::io;
use std::path::Path;

use config::{Direct, PackageEntry};
use misc::md5_digest;

/// Possible errors that may happen when attempting to download Debian packages and source code.
#[derive(Debug, Fail)]
pub enum DownloadError {
    #[fail(display = "unable to download '{}': {}", item, why)]
    Request { item: String, why: reqwest::Error },
    #[fail(display = "unable to open '{}': {}", item, why)]
    File { item: String, why: io::Error },
    #[fail(display = "downloaded archive has an invalid checksum: expected {}, received {}", expected, received)]
    InvalidChecksum { expected: String, received: String }
}

/// Possible messages that may be returned when a download has succeeded.
pub enum DownloadResult {
    Downloaded(u64),
    AlreadyExists,
}

/// Given an item with a URL, download the item if the item does not already exist.
fn download(client: &Client, item: &Direct) -> Result<DownloadResult, DownloadError> {
    eprintln!(" - {}", item.get_name());

    let parent = item.destination();
    let filename = item.file_name();
    let destination = parent.join(filename);

    let dest_result = if destination.exists() {
        if destination.is_file() {
            let mut file = File::open(&destination).map_err(|why| DownloadError::File {
                item: item.get_name().to_owned(),
                why,
            })?;

            if let Some(ref checksum) = item.checksum {
                let digest = md5_digest(file)
                    .map_err(|why| DownloadError::File { item: item.get_name().to_owned(), why })?;

                if &digest == checksum {
                    return Ok(DownloadResult::AlreadyExists);
                }
            } else {
                let capacity = file.metadata().map(|x| x.len()).unwrap_or(0);

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
            }
        }

        File::create(&destination)
    } else {
        create_dir_all(&parent).and_then(|_| File::create(&destination))
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

    let downloaded = response
        .copy_to(&mut dest)
        .map_err(|why| DownloadError::Request {
            item: item.get_name().to_owned(),
            why,
        })?;

    validate(item, &destination).map(|_| DownloadResult::Downloaded(downloaded))
}

fn validate(item: &Direct, dst: &Path) -> Result<(), DownloadError> {
    File::open(dst)
        .map_err(|why| DownloadError::File { item: item.get_name().to_owned(), why})
        .and_then(|file| {
            item.checksum.as_ref().map_or(Ok(()), |checksum| {
                let digest = md5_digest(file)
                    .map_err(|why| DownloadError::File { item: item.get_name().to_owned(), why })?;
                if &digest == checksum {
                    Ok(())
                } else {
                    Err(DownloadError::InvalidChecksum {
                        expected: checksum.to_owned(),
                        received: digest
                    })
                }
            })
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
