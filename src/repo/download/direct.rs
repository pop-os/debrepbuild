use super::request::{self, RequestCompare};
use crate::config::Direct;
use reqwest::Client;
use std::sync::Arc;

/// Possible messages that may be returned when a download has succeeded.
pub enum DownloadResult {
    Downloaded(u64),
}

/// Given an item with a URL, download the item if the item does not already exist.
pub async fn download(
    client: Arc<Client>,
    item: &Direct,
    suite: &str,
    component: &str,
) -> anyhow::Result<DownloadResult> {
    log::info!("checking if {} needs to be downloaded", item.name);

    let mut downloaded = 0;

    for (destination, path) in item
        .get_destinations(suite, component)?
        .into_iter()
        .zip(item.urls.iter())
    {
        let checksum = path.checksum.as_ref().map(|x| x.as_str());
        // If the file is to be repackaged, store it in the assets directory, else the pool.
        let target = destination
            .assets
            .as_ref()
            .map_or(&destination.pool, |x| &x.1);
        downloaded += request::file(
            client.clone(),
            item.name.clone(),
            &destination.url,
            RequestCompare::Checksum(checksum),
            target,
        )
        .await?;
    }

    log::info!("finished downloading {}", &item.name);
    Ok(DownloadResult::Downloaded(downloaded))
}

/// Downloads pre-built Debian packages
pub async fn download_many(
    items: &[Direct],
    suite: &str,
    component: &str,
) -> Vec<anyhow::Result<DownloadResult>> {
    let mut results = Vec::new();

    let client = Arc::new(Client::new());
    for item in items {
        results.push(download(client.clone(), item, suite, component).await);
    }

    results
}
