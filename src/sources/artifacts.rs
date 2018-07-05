use super::SourceError;
use std::borrow::Cow;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub struct LinkedArtifact(PathBuf);

impl Drop for LinkedArtifact {
    fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
}

pub fn link_artifact(src: &Path, dst: &Path) -> Result<LinkedArtifact, SourceError> {
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
                eprintln!("link already found");
                return Ok(LinkedArtifact(dst.to_owned().to_path_buf()));
            } else {
                eprintln!("removing {}", dst.display());
                fs::remove_file(&dst).map_err(|why| SourceError::LinkRemoval { why })?;
            }
        }
    }

    eprintln!("linking {} to {}", src.display(), dst.display());
    fs::hard_link(src, &dst)
        .map(|_| LinkedArtifact(dst.to_owned().to_path_buf()))
        .map_err(|why| SourceError::Link { why })
}

fn resolve_destination<'a>(src: &Path, dst: &'a Path) -> Cow<'a, Path> {
    if dst.is_dir() && src.is_file() {
        Cow::Owned(dst.join(src.file_name().unwrap()))
    } else if dst.is_dir() && src.is_dir() {
        Cow::Owned(dst.join(src.parent().unwrap()))
    } else {
        Cow::Borrowed(dst)
    }
}
