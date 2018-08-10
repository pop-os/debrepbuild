use std::env;
use std::io::{self, Error, ErrorKind};
use std::process::Command;
use std::path::Path;
use walkdir::{DirEntry, WalkDir};
use super::super::pool::{mv_to_pool, ARCHIVES_ONLY};

pub fn generate(suite: &str, component: &str) -> io::Result<()> {
    info!("generating metapackages");
    WalkDir::new("metapackages")
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

    mv_to_pool("metapackages", suite, component, ARCHIVES_ONLY, None)
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

    directory_scope(parent, move || {
        let status = Command::new("equivs-build")
            .arg(filename)
            .status()
            .map_err(|why| Error::new(
                ErrorKind::Other,
                format!("equivs-build failed to run: {}", why)
            ))?;

        if status.success() {
            debug!("equivs-build succeeded for metapackage: {}", path.display());
            Ok(())
        } else {
            Err(status.code().map_or_else(
                || Error::new(ErrorKind::Other, "equivs-build exit status not found"),
                |code| Error::new(ErrorKind::Other, format!("equivs-build exited with status of {}", code))
            ))
        }
    })
}

pub fn directory_scope<T, F: FnMut() -> io::Result<T>>(path: &Path, mut scope: F) -> io::Result<T> {
    let previous = env::current_dir()?;
    env::set_current_dir(path)?;
    let result = scope()?;
    env::set_current_dir(previous)?;
    Ok(result)
}
