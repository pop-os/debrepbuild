use std::{fs, io};
use std::path::{Path, PathBuf};
use config::Config;
use super::version::changelog;
use walkdir::{DirEntry, WalkDir};

pub const SHARED_ASSETS: &str = "assets/share/";
pub const PACKAGE_ASSETS: &str = "assets/packages/";

pub fn create_missing_directories() -> io::Result<()> {
    [SHARED_ASSETS, PACKAGE_ASSETS, "build", "record", "sources", "logs"].iter()
        .map(|dir| if Path::new(dir).exists() { Ok(()) } else { fs::create_dir_all(dir) })
        .collect::<io::Result<()>>()
}

pub fn package_cleanup(config: &Config) -> io::Result<()> {
    if let Some(ref sources) = config.source {
        for source in sources {
            if source.retain != 0 {
                if let Some("changelog") = source.build_on.as_ref().map(|x| x.as_str()) {
                    let cpath = PathBuf::from(["debian/", &source.name, "/changelog"].concat());
                    if cpath.exists() {
                        let keep = changelog(&cpath, 0)?;
                        for (file, version) in locate_files(&source.name, &config.archive) {
                            if keep.iter().any(|x| version.as_str() != x.as_str()) {
                                let path = file.path();
                                info!("cleaning up file at {:?}", path);
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

fn locate_files(name: &str, archive: &str) -> Vec<(DirEntry, String)> {
    let path = PathBuf::from(["pool/", archive, "/"].concat());

    fn matches(entry: &DirEntry, name: &str) -> bool {
        if entry.path().is_dir() {
            false
        } else {
            entry.file_name().to_str().map_or(false, |e| {
                e.split('_').next().map_or(false, |n| n == name)
            })
        }
    }

    WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| matches(e, name))
        .flat_map(|e| e.ok().and_then(|e| {
            e.file_name()
                .to_str()
                .and_then(|e| e.split('_').nth(1).map(|v| v.to_owned()))
                .map(|v| (e, v))
        })).collect()
}
