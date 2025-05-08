use crate::checksum::hasher;
use reqwest::Client;
use sha2::Sha256;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;

const ATTEMPTS: u8 = 3;

pub enum RequestCompare<'a> {
    Checksum(Option<&'a str>),
    SizeAndModification(u64, Option<i64>),
}

pub async fn file<'a>(
    client: Arc<Client>,
    _name: String,
    url: &str,
    compare: RequestCompare<'a>,
    path: &Path,
) -> anyhow::Result<u64> {
    let mut tries = 0;

    loop {
        let mut file = if path.exists() {
            let mut requires_download = true;

            match compare {
                RequestCompare::Checksum(Some(checksum)) => {
                    let digest = hasher::<Sha256, File>(File::open(path)?)?;
                    requires_download = digest != checksum;
                }
                RequestCompare::SizeAndModification(length, mtime) => {
                    let file = File::open(path)?;
                    let metadata = file.metadata()?;
                    if metadata.len() == length {
                        if let Some(modified) = mtime {
                            if modified == metadata.mtime() {
                                requires_download = false;
                            }
                        } else {
                            requires_download = false;
                        }
                    }
                }
                _ => (),
            }

            if !requires_download {
                return Ok(0);
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

        log::info!("downloading package to {}", path.display());

        let mut response = client.get(url).send().await?;

        while let Some(chunk) = response.chunk().await? {
            file.write(&chunk)?;
        }

        file.flush()?;

        log::info!("finished downloading {}", path.display());
        if let RequestCompare::Checksum(Some(checksum)) = compare {
            let digest = hasher::<Sha256, File>(File::open(path)?)?;
            if digest == checksum {
                return Ok(0);
            } else {
                log::error!("checksum does not match for {}, removing.", path.display());
                fs::remove_file(&path)?;

                if tries == ATTEMPTS {
                    return Err(anyhow::anyhow!(
                        "checksum does not match for {}",
                        path.display()
                    ));
                }

                tries += 1;
            }
        } else if let RequestCompare::SizeAndModification(_length, Some(mtime)) = compare {
            let (atime, _) = utime::get_file_times(path)?;
            utime::set_file_times(path, atime, mtime)?;
            return Ok(0);
        } else {
            return Ok(0);
        }
    }
}
