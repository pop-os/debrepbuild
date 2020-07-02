use std::{fs, io};
use std::path::{Path, PathBuf};
use config::Config;
use debian::DEB_SOURCE_EXTENSIONS;
use super::version::changelog;
use walkdir::{DirEntry, WalkDir};

pub const CACHED_ASSETS: &str = "assets/cache/";
pub const SHARED_ASSETS: &str = "assets/share/";
pub const PACKAGE_ASSETS: &str = "assets/packages/";

pub fn create_missing_directories(suite: &str) -> io::Result<()> {
    let record = ["record/", suite].concat();
    let logs = ["logs/", suite].concat();
    let build = ["build/", suite].concat();
    [CACHED_ASSETS, SHARED_ASSETS, PACKAGE_ASSETS, &build, &record, &logs].iter()
        .map(|dir| if Path::new(dir).exists() { Ok(()) } else { fs::create_dir_all(dir) })
        .collect::<io::Result<()>>()
}

pub fn package_cleanup(config: &Config) -> io::Result<()> {
    let path = PathBuf::from(["repo/pool/", &config.archive, "/", &config.default_component].concat());
    for entry in WalkDir::new(path).min_depth(3).max_depth(3).into_iter().filter_map(|x| x.ok()) {
        let path = entry.path();
        if let Some(filename) = path.file_name().and_then(|x| x.to_str()) {
            if !config.package_exists(filename) {
                info!("removing files at {:?}", path);
                fs::remove_dir_all(path)?;
            }
        }
    }

    if let Some(ref sources) = config.source {
        for source in sources {
            if source.retain != 0 {
                if let Some("changelog") = source.build_on.as_deref() {
                    let cpath = PathBuf::from(["debian/", &config.archive, "/", &source.name, "/changelog"].concat());
                    if cpath.exists() {
                        let keep = changelog(&cpath, source.retain)?;
                        for (file, version) in locate_files(&source.name, &config.archive) {
                            if !keep.iter().any(|x| version.as_str() == x.as_str()) {
                                let path = file.path();
                                info!("removing file at {:?}", path);
                                fs::remove_file(path)?;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn build_directories(suite: &str) -> io::Result<()> {
    let path = PathBuf::from(["build/", suite].concat());
    if path.exists() {
        debug!("removing {}", path.display());
        fs::remove_dir_all(&path)?;
    }

    fs::create_dir_all(&path)
}

pub fn remove(packages: &[&str], suite: &str, component: &str) -> io::Result<()> {
    let path = PathBuf::from(["repo/pool/", suite, "/", component].concat());
    for entry in WalkDir::new(path).min_depth(3).max_depth(3).into_iter().filter_map(|x| x.ok()) {
        let path = entry.path();
        if let Some(filename) = path.file_name().and_then(|x| x.to_str()) {
            if packages.contains(&filename) {
                info!("removing files at {:?}", path);
                fs::remove_dir_all(path)?;
            }
        }
    }

    Ok(())
}

fn locate_files(name: &str, archive: &str) -> Vec<(DirEntry, String)> {
    let path = PathBuf::from(["repo/pool/", archive, "/"].concat());

    fn matches(entry: &DirEntry, name: &str) -> bool {
        if entry.path().is_dir() {
            true
        } else {
            entry.file_name().to_str().map_or(false, |e| {
                e.split('_').next().map_or(false, |n| {
                    n == name || (n.ends_with("-dbgsym") && &n[..n.len() - 7] == name)
                })
            })
        }
    }

    WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| matches(e, name))
        .flat_map(|e| e.ok().and_then(|e| if e.path().is_file() { Some(e) } else { None }))
        .flat_map(|e| e.file_name()
            .to_str()
            .and_then(|e| e.split('_').nth(1).map(|v| get_version(v).to_owned()))
            .map(|v| (e, v))
        ).collect()
}

fn get_version(e: &str) -> &str {
    for ext in DEB_SOURCE_EXTENSIONS {
        if e.ends_with(ext) {
            return &e[..e.len() - ext.len()];
        }
    }

    e
}
