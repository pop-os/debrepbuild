use std::ffi::CString;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::process::Command;
use reqwest::Client;

use libc;
use sha2::{Sha256, Digest};
use walkdir::{DirEntry, WalkDir};

pub fn walk_debs(path: &Path) -> Box<Iterator<Item = DirEntry>> {
    fn is_deb(entry: &DirEntry) -> bool {
        if entry.path().is_dir() {
            true
        } else {
            entry.file_name().to_str().map_or(false, |e| e.ends_with(".deb"))
        }
    }

    Box::new(WalkDir::new(path).into_iter().filter_entry(|e| is_deb(e)).flat_map(|e| e.ok()))
}

pub fn match_deb(entry: &DirEntry, packages: &[String]) -> Option<(String, usize)> {
    let path = entry.path();
    if path.is_dir() {
        return None
    }

    entry.file_name().to_str().and_then(|package| {
        let package = &package[..package.find('_').expect("debian package lacks _ character")];

        packages.iter().position(|x| x.as_str() == package)
            .and_then(|pos| path.to_str().map(|path| (path.to_owned(), pos)))
    })
}

pub fn unlink(link: &Path) -> io::Result<()> {
    CString::new(link.to_path_buf().into_os_string().into_vec())
        .map_err(|why| io::Error::new(io::ErrorKind::InvalidInput, format!("{}", why)))
        .and_then(|link| match unsafe { libc::unlink(link.as_ptr()) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error())
        })
}

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

pub fn download_file(client: &Client, url: &str, path: &Path, checksum: &str) -> io::Result<u64> {
    let mut file = if path.exists() {
        let digest = sha2_256_digest(File::open(path)?)?;
        if &digest == checksum {
            return Ok(0);
        }

        File::open(path)?
    } else {

        File::create(path)?
    };

    let downloaded = client
        .get(url)
        .send()
        .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("reqwest get failed: {}", why)))?
        .copy_to(&mut file)
        .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("reqwest copy failed: {}", why)))?;

    let digest = sha2_256_digest(File::open(path)?)?;
    if &digest == checksum {
        Ok(downloaded)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("checksum does not match for {}", path.display())
        ))
    }
}

pub fn sha2_256_digest(file: File) -> io::Result<String> {
    let mut hasher = Sha256::default();
    let data = &mut BufReader::new(file);
    loop {
        let read = {
            let buffer = data.fill_buf()?;
            if buffer.len() == 0 { break }
            hasher.input(buffer);
            buffer.len()
        };

        data.consume(read);
    }

    Ok(format!("{:x}", hasher.result()))
}

// NOTE: The following functions are implemented within Rust's standard in 1.26.0

fn initial_buffer_size(file: &File) -> usize {
    file.metadata().ok().map_or(0, |x| x.len()) as usize
}

pub fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut string = String::with_capacity(initial_buffer_size(&file));
    file.read_to_string(&mut string)?;
    Ok(string)
}

pub fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(initial_buffer_size(&file));
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
    File::create(path)?.write_all(contents.as_ref())
}
