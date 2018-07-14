use std::path::Path;
use std::{io, fs};
use std::process::Command;

pub fn rsync(src: &Path, dst: &Path) -> io::Result<()> {
    info!("rsyncing {} to {}", src.display(), dst.display());

    if src.is_dir() {
        fs::create_dir_all(src)?;
    }

    Command::new("rsync")
        .arg("-avz")
        .arg(src)
        .arg(dst)
        .status()
        .and_then(|x| if x.success() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "tar command failed"))
        })
}
