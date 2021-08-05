use std::borrow::Cow;
use std::{fs, io};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use crate::misc::unlink;

pub struct LinkedArtifact(PathBuf);

pub struct LinkError {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub why: io::Error
}

impl LinkError {
    pub fn new(src: &Path, dst: &Path, why: io::Error) -> LinkError {
        LinkError { src: src.to_path_buf(), dst: dst.to_path_buf(), why }
    }
}

impl Drop for LinkedArtifact {
    fn drop(&mut self) {
        let _ = unlink(&self.0);
    }
}

pub fn link_artifact(src: &Path, dst: &Path) -> Result<LinkedArtifact, LinkError> {
    let dst = resolve_destination(src, dst);

    if let Some(dst_ino) = dst.as_ref().metadata().ok().map(|m| m.ino()) {
        if let Some(src_ino) = src.metadata().ok().map(|m| m.ino()) {
            if src_ino == dst_ino
                && !dst
                    .as_ref()
                    .symlink_metadata()
                    .unwrap()
                    .file_type()
                    .is_symlink()
            {
                return Ok(LinkedArtifact(dst.to_owned().to_path_buf()));
            } else {
                info!("removing link at {}", dst.display());
                unlink(&dst).map_err(|why| LinkError::new(src, &dst, why))?;
            }
        }
    }

    info!("linking {} to {}", src.display(), dst.display());
    fs::hard_link(src, &dst)
        .map(|_| LinkedArtifact(dst.to_owned().to_path_buf()))
        .map_err(|why| LinkError::new(src, &dst, why))
}

fn resolve_destination<'a>(mut src: &'a Path, dst: &'a Path) -> Cow<'a, Path> {
    let src_is_file = src.is_file();
    for component in dst.components().map(|comp| comp.as_os_str()) {
        if let Ok(path) = src.strip_prefix("/") {
            src = path;
        }

        if let Ok(path) = src.strip_prefix(component) {
            src = path;
        } else {
            break
        }
    }

    if dst.is_dir() && src_is_file {
        Cow::Owned(dst.join(src.file_name().unwrap()))
    } else if dst.is_dir() && src.is_dir() {
        Cow::Owned(dst.join(src.parent().unwrap()))
    } else {
        Cow::Borrowed(dst)
    }
}
