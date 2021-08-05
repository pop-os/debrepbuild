use apt_repo_crawler::{filename_from_url, AptCrawler, AptEntry, AptPackage};
use crate::config::Repo;
use crossbeam_channel::bounded;
use deb_version;
use crate::debian::gen_filename;
use reqwest::Client;
use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::Arc;
use super::request::{self, RequestCompare};

pub async fn download(repos: Vec<Repo>, suite: String, component: String) -> anyhow::Result<()> {
    let (in_tx, in_rx) = bounded::<AptEntry>(64);
    let (out_tx, out_rx) = bounded::<(String, String, RequestCompare, PathBuf)>(64);

    std::thread::spawn(move || {
        for repo in repos {
            info!("fetching packages from {}", repo.repo);
            let crawler = AptCrawler::new(repo.repo.clone())
                .filter(Arc::new(repo.clone()))
                .crawl();

            for package in crawler {
                let _ = in_tx.send(package);
            }
        }
    });

    std::thread::spawn(move || {
        // Sends data required by the file requester to the output channel.
        let send_func = |file: AptEntry| -> bool {
            if let Ok(desc) = AptPackage::from_str(filename_from_url(file.url.as_str())) {
                let _ = out_tx.send((
                    desc.name.to_owned(),
                    file.url.as_str().to_owned(),
                    RequestCompare::SizeAndModification(
                        file.length,
                        file.modified.map(|m| m.timestamp())
                    ),
                    get_destination(desc, &suite, &component)
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


    let client = Arc::new(Client::new());
    for (name, url, compare, dest) in out_rx {
        request::file(client.clone(), name, &url, compare, &dest).await?;
    }

    Ok(())
}

fn get_destination(desc: AptPackage, suite: &str, component: &str) -> PathBuf {
    let dst = match desc.extension {
        "tar.gz" | "tar.xz" | "tar.zst" | "dsc" => ["/", component, "/source/"].concat(),
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
