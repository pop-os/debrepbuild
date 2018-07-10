use std::{fs, io};
use std::path::Path;
use std::process::Command;

pub fn extract(src: &Path, dst: &Path) -> io::Result<()> {
    match src.file_name().and_then(|x| x.to_str()) {
        Some(filename) => {
            if filename.ends_with(".zip") {
                unzip(src, dst)
            } else if filename.ends_with(".tar.gz") || filename.ends_with(".tar.xz") {
                untar(src, dst)
            } else {
                unimplemented!()
            }
        }
        None => unimplemented!()
    }
}

pub fn unzip(path: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }

    fs::create_dir_all(dst)
        .and_then(|_| Command::new("unzip")
            .arg(path)
            .arg("-d")
            .arg(dst)
            .status()
            .and_then(|x| if x.success() {
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::Other, "tar command failed"))
            })
        )
}

pub fn untar(path: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }

    fs::create_dir_all(dst)
        .and_then(|_| Command::new("tar")
            .arg("-xvf")
            .arg(path)
            .arg("-C")
            .arg(dst)
            .args(&["--strip-components", "1"])
            .status()
            .and_then(|x| if x.success() {
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::Other, "tar command failed"))
            })
        )
}
