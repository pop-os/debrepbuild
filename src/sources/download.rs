use super::build::build;
use super::SourceError;
use config::Source;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Downloads source code repositories and builds them in parallel.
pub fn parallel(items: &[Source], branch: &str) -> Vec<Result<(), SourceError>> {
    eprintln!("downloading sources in parallel");
    items
        .par_iter()
        .map(|item| match item.path {
            Some(ref path) => build(item, Path::new(path), branch),
            None => match item.cvs.as_str() {
                "git" => download_git(item, branch),
                cvs => Err(SourceError::UnsupportedCVS {
                    cvs: cvs.to_owned(),
                }),
            }
        })
        .collect()
}

/// Downloads the source repository via git, then attempts to build it.
fn download_git(item: &Source, branch: &str) -> Result<(), SourceError> {
    assert!(item.url.is_some());

    let name: String = {
        let url = item.url.as_ref().unwrap();
        url.split_at(url.rfind('/').unwrap() + 1)
            .1
            .replace(".git", "")
    };

    let path = PathBuf::from(["sources/", &name].concat());

    if path.exists() {
        eprintln!("pulling {}", name);
        let exit_status = Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(&["pull", "origin", "master"])
            .status()
            .map_err(|why| SourceError::GitRequest {
                item: name.to_owned(),
                why,
            })?;

        if !exit_status.success() {
            return Err(SourceError::GitFailed);
        }
    } else {
        eprintln!("cloning {}", name);
        let exit_status = Command::new("git")
            .args(&["-C", "sources", "clone", &item.url.as_ref().unwrap()])
            .status()
            .map_err(|why| SourceError::GitRequest {
                item: name.to_owned(),
                why,
            })?;

        if !exit_status.success() {
            return Err(SourceError::GitFailed);
        }
    }

    build(item, &path, branch)
}
