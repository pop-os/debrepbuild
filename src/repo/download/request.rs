use std::{fs, io};
use std::fs::File;
use std::path::Path;
use checksum::hasher;
use reqwest::Client;
use sha2::Sha256;

const ATTEMPTS: u8 = 3;

pub fn file(client: &Client, url: &str, checksum: Option<&str>, path: &Path) -> io::Result<u64> {
    let mut tries = 0;
    loop {
        let mut file = if path.exists() {
            if let Some(checksum) = checksum {
                let digest = hasher::<Sha256, File>(File::open(path)?)?;
                if digest == checksum {
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

        let digest = hasher::<Sha256, File>(File::open(path)?)?;
        if let Some(checksum) = checksum {
            if digest == checksum {
                return Ok(downloaded);
            } else {
                error!("checksum does not much for {}, removing.", path.display());
                fs::remove_file(&path)?;

                if tries == ATTEMPTS {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("checksum does not match for {}", path.display())
                    ));
                }

                tries += 1;
            }
        } else {
            return Ok(downloaded);
        }
    }
}
