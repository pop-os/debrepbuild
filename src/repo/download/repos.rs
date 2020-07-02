use apt_repo_crawler::{filename_from_url, AptCrawler, AptEntry, AptPackage};
use config::Repo;
use crossbeam_channel::bounded;
use deb_version;
use debian::gen_filename;
use rayon::{scope, ThreadPoolBuilder};
use rayon::prelude::*;
use reqwest::Client;
use std::cmp::Ordering;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
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

                let mut files: Vec<AptEntry> = Vec::new();
                let mut names: Vec<String> = Vec::new();
                let mut versions: Vec<String> = Vec::new();

                enum Insert {
                    Append(String, String),
                    Update(usize, String)
                }

                for file in in_rx {
                    let mut update = None;
                    if let Ok(desc) = AptPackage::from_str(filename_from_url(file.url.as_str())) {
                        if let Some(position) = names.iter().position(|name| name == desc.name) {
                            if deb_version::compare_versions(&versions[position], desc.version) == Ordering::Less {
                                update = Some(Insert::Update(position, desc.version.to_owned()));
                            }
                        } else {
                            update = Some(Insert::Append(desc.name.to_owned(), desc.version.to_owned()));
                        }
                    }

                    match update {
                        Some(Insert::Append(name, version)) => {
                            files.push(file);
                            names.push(name);
                            versions.push(version);
                        },
                        Some(Insert::Update(pos, version)) => {
                            files[pos] = file;
                            versions[pos] = version;
                        },
                        None => (),
                    }
                }

                for entry in files {
                    send_func(entry);
                }
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
