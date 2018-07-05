use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use md5;

pub fn md5_digest(file: File) -> io::Result<String> {
    let mut context = md5::Context::new();
    let data = &mut BufReader::new(file);
    loop {
        let read = {
            let buffer = data.fill_buf()?;
            if buffer.len() == 0 { break }
            context.consume(buffer);
            buffer.len()
        };

        data.consume(read);
    }

    Ok(format!("{:x}", context.compute()))
}

pub fn extract_tar(path: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }

    if fs::create_dir_all(dst)
        .and_then(|_| Command::new("tar")
            .arg("-xvf")
            .arg(path)
            .arg("-C")
            .arg(dst)
            .status()
            .map(|x| x.success())
        )?
    {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "tar command failed"))
    }

}

pub fn mv_to_pool<P: AsRef<Path>>(path: P) -> io::Result<()> {
    pool(path.as_ref(), |src, dst| fs::rename(src, dst))
}

pub fn cp_to_pool<P: AsRef<Path>>(path: P) -> io::Result<()> {
    pool(path.as_ref(), |src, dst| fs::copy(src, dst).map(|_| ()))
}

fn pool<F: Fn(&Path, &Path) -> io::Result<()>>(path: &Path, action: F) -> io::Result<()> {
    for entry in path.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        let filename = path.file_name().and_then(|x| x.to_str());
        let filestem = path.file_stem().and_then(|x| x.to_str());
        if let (Some(filename), Some(filestem)) = (filename, filestem) {
            let package = &filename[..filename.find('_').unwrap_or(0)];
            let arch = &filestem[filestem.rfind('_').unwrap_or(0) + 1..];
            let destination = PathBuf::from(
                ["pool/main/binary-", arch, "/", &package[0..1], "/", package].concat(),
            );

            fs::create_dir_all(&destination)?;
            action(&path, &destination.join(filename))?;
        }
    }

    Ok(())
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
