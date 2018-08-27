use rayon::prelude::*;
use reqwest::Client;
use std::io;
use config::Direct;
use super::request;

/// Possible messages that may be returned when a download has succeeded.
pub enum DownloadResult {
    Downloaded(u64)
}

/// Given an item with a URL, download the item if the item does not already exist.
pub fn download(client: &Client, item: &Direct, suite: &str, component: &str) -> io::Result<DownloadResult> {
    info!("checking if {} needs to be downloaded", item.name);

    let mut downloaded = 0;

    for (destination, path) in item.get_destinations(suite, component)?.into_iter().zip(item.urls.iter()) {
        let checksum = path.checksum.as_ref().map(|x| x.as_str());
        // If the file is to be repackaged, store it in the assets directory, else the pool.
        let target = destination.assets.as_ref().map_or(&destination.pool, |x| &x.1);
        debug!("download {}? target {:?}", &item.name, target);
        downloaded += request::file(client, &destination.url, checksum, target)?;
    }

    info!("finished downloading {}", &item.name);
    Ok(DownloadResult::Downloaded(downloaded))
}

/// Downloads pre-built Debian packages in parallel
pub fn parallel(items: &[Direct], suite: &str, component: &str) -> Vec<io::Result<DownloadResult>> {
    let client = Client::new();
    items
        .par_iter()
        .map(|item| download(&client, item, suite, component))
        .collect()
}
