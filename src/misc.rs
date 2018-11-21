use std::ffi::CString;
use std::fs::{self, File};
use std::io::{self, Error, ErrorKind, Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use debian::DEB_SOURCE_EXTENSIONS;

use libc;
use walkdir::{DirEntry, WalkDir};

pub const INCLUDE_DDEB: u8 = 1;
pub const INCLUDE_SRCS: u8 = 2;

pub fn filename_from_url(url: &str) -> &str {
    &url[url.rfind('/').map_or(0, |x| x + 1)..]
}

// Recursively removes directories from the given path, if the directories or their subdirectories
// are empty.
pub fn remove_empty_directories_from(directory: &Path) -> io::Result<bool> {
    let mut empty = true;
    let entries = directory.read_dir().map_err(|why| Error::new(
        ErrorKind::Other,
        format!("unable to read directory at {:?}: {}", directory, why)
    ))?;

    for entry in entries {
        let entry = entry.map_err(|why| Error::new(
            ErrorKind::Other,
            format!("bad entry in {:?}: {}", directory, why)
        ))?;

        let path = entry.path();
        if path.is_dir() {
            if !remove_empty_directories_from(&path)? {
                empty = false;
            }
        } else {
            return Ok(false);
        }
    }

    if empty {
        info!("removing {} because it is empty", directory.display());
        fs::remove_dir(directory).map_err(|why| Error::new(
            ErrorKind::Other,
            format!("unable to remove entry at {:?}: {}", directory, why)
        ))?;
    }

    Ok(empty)
}

pub fn is_deb(entry: &DirEntry, flags: u8) -> bool {
    entry.file_name().to_str().map_or(false, |e| {
        e.ends_with(".deb") || {
            if flags & INCLUDE_DDEB != 0 { e.ends_with(".ddeb") } else { false }
        } || {
            if flags & INCLUDE_SRCS != 0 {
                DEB_SOURCE_EXTENSIONS.into_iter().any(|ext| e.ends_with(ext))
            } else {
                false
            }
        }
    })
}

pub fn walk_debs(path: &Path, ddeb: bool) -> Box<Iterator<Item = DirEntry>> {
    Box::new(
        WalkDir::new(path)
            .into_iter()
            .filter_entry(move |e| if e.path().is_dir() { true } else { is_deb(e, ddeb as u8) })
            .flat_map(|e| e.ok())
    )
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

pub fn copy_here<S>(source: S) -> io::Result<()>
    where S: AsRef<Path>,
{
    for entry in source.as_ref().read_dir()? {
        let entry = entry?;
        if entry.path().is_file() {
            let source = &entry.path();
            if let Some(dest) = source.file_name() {
                eprintln!("copying {:?} to {:?}", source, dest);
                io::copy(&mut File::open(source)?, &mut File::create(dest)?)?;
            }
        }
    }

    Ok(())
}

pub fn unlink(link: &Path) -> io::Result<()> {
    CString::new(link.to_path_buf().into_os_string().into_vec())
        .map_err(|why| io::Error::new(io::ErrorKind::InvalidInput, format!("{}", why)))
        .and_then(|link| match unsafe { libc::unlink(link.as_ptr()) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error())
        })
}

pub fn get_arch_from_stem(stem: &str) -> &str {
    if let Some(arch) = ["amd64", "i386"].iter().find(|&x| stem.ends_with(x)) {
        return arch;
    }

    let arch = &stem[stem.rfind('_').unwrap_or(0) + 1..];
    arch.find('-').map_or(arch, |pos| &arch[..pos])
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

pub fn copy<S: AsRef<Path>, D: AsRef<Path>>(src: S, dst: D) -> io::Result<()> {
    io::copy(&mut File::open(src)?, &mut File::create(dst)?)?;
    Ok(())
}
