use rayon::prelude::*;
use reqwest::Client;
use std::io;
use std::path::PathBuf;

use config::Direct;
use misc;

/// Possible messages that may be returned when a download has succeeded.
pub enum DownloadResult {
    Downloaded(u64),
    AlreadyExists,
}

/// Given an item with a URL, download the item if the item does not already exist.
pub fn download(client: &Client, item: &Direct, branch: &str) -> io::Result<DownloadResult> {
    info!("downloading package named {}", item.name);

    fn gen_filename(name: &str, version: &str, arch: &str, ext: &str) -> String {
        [name, if ext == "ddeb" { "-dbgsym_" } else { "_" }, version, "_", arch, ".", ext].concat()
    }

    let mut downloaded = 0;
    for file_item in &item.urls {
        let destination = {
            let name: &str = file_item.name.as_ref().map_or(&item.name, |x| &x);
            let file = &file_item.url[file_item.url.rfind('/').unwrap_or(0) + 1..];

            let ext_pos = {
                let mut ext_pos = file.rfind('.').unwrap_or_else(|| file.len()) + 1;
                match &file[ext_pos..] {
                    "gz" | "xz" => match &file[ext_pos - 4..ext_pos - 1] {
                        "tar" => ext_pos = ext_pos - 4,
                        _ => ()
                    }
                    _ => ()
                }
                ext_pos
            };

            let extension = &file[ext_pos..];
            let arch = match file_item.arch.as_ref() {
                Some(ref arch) => arch.as_str(),
                None => misc::get_arch_from_stem(&file[..ext_pos - 1]),
            };

            let filename = &gen_filename(name, &item.version, arch, extension);

            let dst = match extension {
                "tar.gz" | "tar.xz" | "dsc" => "/main/source/".into(),
                _ => ["/main/binary-", arch, "/"].concat()
            };

            PathBuf::from(
                [ "repo/pool/", branch, &dst, &name[0..1], "/", name, "/", &filename ].concat()
            )
        };

        let checksum = file_item.checksum.as_ref().map(|x| x.as_str());
        downloaded += misc::download_file(client, &file_item.url, checksum, &destination)?;
    }

    info!("finished downloading {}", &item.name);
    Ok(DownloadResult::Downloaded(downloaded))
}

/// Downloads pre-built Debian packages in parallel
pub fn parallel(items: &[Direct], branch: &str) -> Vec<io::Result<DownloadResult>> {
    let client = Client::new();
    items
        .par_iter()
        .map(|item| download(&client, item, branch))
        .collect()
}
