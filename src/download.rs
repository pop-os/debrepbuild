use std::{
    fs::{create_dir_all, File},
    io,
};

use rayon::prelude::*;
use reqwest::{self, header::ContentLength, Client, Response};

use sources::Direct;

#[derive(Debug, Fail)]
pub enum DownloadError {
    #[fail(display = "unable to download '{}': {}", item, why)]
    Request { item: String, why:  reqwest::Error },
    #[fail(display = "unable to open '{}': {}", item, why)]
    File { item: String, why:  io::Error },
}

pub enum DownloadResult {
    Downloaded(u64),
    AlreadyExists,
}

fn download(client: &Client, item: &Direct) -> Result<DownloadResult, DownloadError> {
    eprintln!("downloading {}", item.name);

    let parent = item.destination();
    let filename = item.file_name();
    let destination = parent.join(&filename);

    let dest_result = if destination.exists() {
        let mut capacity = File::open(&destination)
            .and_then(|file| file.metadata().map(|x| x.len()))
            .unwrap_or(0);

        let response = client
            .head(&item.url)
            .send()
            .map_err(|why| DownloadError::Request {
                item: item.name.clone(),
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
        item: item.name.clone(),
        why,
    })?;

    let mut response = client
        .get(&item.url)
        .send()
        .map_err(|why| DownloadError::Request {
            item: item.name.clone(),
            why,
        })?;

    response
        .copy_to(&mut dest)
        .map(|x| DownloadResult::Downloaded(x))
        .map_err(|why| DownloadError::Request {
            item: item.name.clone(),
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

pub fn parallel(items: &[Direct]) -> Vec<Result<DownloadResult, DownloadError>> {
    eprintln!("downloading direct download packages in parallel");
    let client = Client::new();
    items
        .par_iter()
        .map(|item| download(&client, item))
        .collect()
}
