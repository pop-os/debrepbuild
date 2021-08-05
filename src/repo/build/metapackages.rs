use std::env;
use std::fs;
use std::io::{self, Error, ErrorKind};
use crate::command::Command;
use std::path::Path;
use walkdir::{DirEntry, WalkDir};
use super::super::pool::{mv_to_pool, ARCHIVES_ONLY};

pub fn generate(suite: &str, component: &str) -> io::Result<()> {
    let metapackages = &Path::new("metapackages").join(suite);
    if !metapackages.exists() {
        return Ok(());
    }

    info!("removing any leftover deb archives in the metapackages directory");
    for entry in metapackages.read_dir()? {
        let entry = entry?;
        if entry.file_name().to_str().map_or(false, |e| e.ends_with(".deb")) {
            fs::remove_file(entry.path())?;
        }
    }

    info!("generating metapackages");
    WalkDir::new(metapackages)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_entry(|e| is_cfg(e))
        .map(|e| {
            e.map_err(|why| Error::new(
                ErrorKind::Other,
                format!("entry in directory walk had an error: {}", why)
            )).and_then(|ref x| inner_generate(x))
        })
        .collect::<io::Result<()>>()?;

    mv_to_pool(&metapackages, suite, component, ARCHIVES_ONLY, None)
}

fn is_cfg(entry: &DirEntry) -> bool {
    !entry.path().is_dir() && entry.file_name().to_str().map_or(false, |e| e.ends_with(".cfg"))
}

fn inner_generate(entry: &DirEntry) -> io::Result<()> {
    let filename = entry.file_name();
    let path = entry.path();

    info!("generating metapackage at {}", path.display());
    let parent = path.parent().ok_or_else(|| Error::new(
        ErrorKind::NotFound,
        format!("parent path not found from {}", path.display())
    ))?;

    directory_scope(parent, move || Command::new("equivs-build").arg(filename).run())
}

pub fn directory_scope<T, F: FnMut() -> io::Result<T>>(path: &Path, mut scope: F) -> io::Result<T> {
    let previous = env::current_dir()?;
    env::set_current_dir(path)?;
    let result = scope()?;
    env::set_current_dir(previous)?;
    Ok(result)
}
