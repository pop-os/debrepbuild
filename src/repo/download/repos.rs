use apt_repo_crawler::{filename_from_url, AptCrawler, AptEntry, AptPackage};
use config::Repo;
use debian::gen_filename;
use rayon::prelude::*;
use rayon::{scope, ThreadPoolBuilder};
use reqwest::Client;
use std::cmp::Ordering;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use crossbeam_channel::bounded;
use super::request::{self, RequestCompare};

pub fn download(repos: &[Repo], suite: &str, component: &str) -> io::Result<()> {
    let mut result = Ok(());
    let (in_tx, in_rx) = bounded::<AptEntry>(64);
    let (out_tx, out_rx) = bounded::<(String, String, RequestCompare, PathBuf)>(64);

    {
        let result = &mut result;
        scope(|s| {
            // Thread for crawling and scraping the apt repo's directory list.
            s.spawn(move |_| {
                for repo in repos {
                    info!("fetching packages from {}", repo.repo);
                    let crawler = AptCrawler::new(repo.repo.clone())
                        .filter(Arc::new(repo.clone()))
                        .crawl();

                    for package in crawler {
                        in_tx.send(package);
                    }
                }
            });

            // Thread for filtering packages to download, based on filter patterns and old-ness.
            s.spawn(move |_| {
                let mut current = String::new();
                let mut latest: Option<AptEntry> = None;

                // Sends data required by the file requester to the output channel.
                let send_func = |file: AptEntry| -> bool {
                    if let Ok(desc) = AptPackage::from_str(filename_from_url(file.url.as_str())) {
                        out_tx.send((
                            desc.name.to_owned(),
                            file.url.as_str().to_owned(),
                            RequestCompare::SizeAndModification(
                                file.length,
                                file.modified.map(|m| m.timestamp())
                            ),
                            get_destination(desc, suite, component)
                        ));
                    }

                    true
                };

                for file in in_rx {
                    let mut send = false;
                    let mut update = false;
                    if let Ok(desc) = AptPackage::from_str(filename_from_url(file.url.as_str())) {
                        // `current` will differ from `desc.name` when a new package name is being scraped.
                        if current != desc.name {
                            current = desc.name.to_owned();
                            update = if current == "" {
                                true
                            } else {
                                send = true;
                                match latest {
                                    Some(ref prev_file) => {
                                        AptPackage::from_str(filename_from_url(prev_file.url.as_str())).ok()
                                            .map(|ref prev_desc| prev_desc.version.cmp(desc.version) != Ordering::Less)
                                            .unwrap_or(false)
                                    }
                                    None => true
                                }
                            };
                        }
                    }

                    // If the probed package is newer than the current one, or the current
                    // package does not exist, this will update the latest detected package.
                    if update {
                        latest = Some(file);
                    }

                    // If it's been designated that this package will be submitted to the
                    // file requester, this will do so.
                    if send {
                        if let Some(file) = latest.take() {
                            if ! send_func(file) { break }
                        }
                    }
                }

                // If the channel was closed while a package has not yet been submitted, do so.
                if let Some(file) = latest { send_func(file); }
            });

            // Use a thread pool to ensure only up to 8 files are downloaded at the same time.
            let thread_pool = ThreadPoolBuilder::new()
                .num_threads(8)
                .build()
                .expect("failed to build thread pool");

            thread_pool.install(move || {
                // Main thread fetches packages in parallel
                let client = Arc::new(Client::new());
                *result = out_rx
                    .into_iter()
                    .par_bridge()
                    .map(|(name, url, compare, dest)| {
                        let client = client.clone();
                        request::file(client, name, &url, compare, &dest)?;
                        Ok(())
                    })
                    .collect::<io::Result<()>>();
            });
        });
    }

    result
}

fn get_destination(desc: AptPackage, suite: &str, component: &str) -> PathBuf {
    let dst = match desc.extension {
        "tar.gz" | "tar.xz" | "dsc" => ["/", component, "/source/"].concat(),
        _ => ["/", component, "/binary-", desc.arch, "/"].concat()
    };

    let filename = gen_filename(&desc.name, &desc.version, &desc.arch, &desc.extension);
    let name = if desc.name.ends_with("-dbg") {
        &desc.name[..desc.name.len()-4]
    } else if desc.name.ends_with("-dbgsym") {
        &desc.name[..desc.name.len()-7]
    } else {
        &desc.name
    };

    PathBuf::from(["repo/pool/", suite, &dst, &name[0..1], "/", &name, "/", &filename].concat())
}