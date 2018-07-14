use std::{fs, io};
use std::fs::File;
use std::path::Path;
use super::checksum::sha2_256_digest;
use reqwest::Client;

pub fn file(client: &Client, url: &str, checksum: Option<&str>, path: &Path) -> io::Result<u64> {
    let mut file = if path.exists() {
        if let Some(checksum) = checksum {
            let digest = sha2_256_digest(File::open(path)?)?;
            if &digest == checksum {
                info!("{} is already downloaded", path.display());
                return Ok(0);
            }
        }

        fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)?
    } else {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        File::create(path)?
    };

    info!("downloading file from {} to {}", url, path.display());
    let downloaded = client
        .get(url)
        .send()
        .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("reqwest get failed: {}", why)))?
        .copy_to(&mut file)
        .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("reqwest copy failed: {}", why)))?;

    let digest = sha2_256_digest(File::open(path)?)?;
    if let Some(checksum) = checksum {
        if &digest == checksum {
            Ok(downloaded)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("checksum does not match for {}", path.display())
            ))
        }
    } else {
        Ok(downloaded)
    }
}
