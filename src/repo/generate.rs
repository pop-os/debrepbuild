use ar;
use config::Config;
use libflate::gzip::Decoder as GzDecoder;
use misc;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tar;
use xz2::read::XzDecoder;

use super::compress::*;

/// Generates the binary files from Debian packages that exist within the pool, using
/// `apt-ftparchive`
pub(crate) fn binary_files(config: &Config, dist_base: &str, pool_base: &str) -> io::Result<()> {
    info!("generating binary files");
    let branch = PathBuf::from([dist_base, "/main/"].concat());

    for directory in fs::read_dir(pool_base)? {
        let entry = directory?;
        let arch = entry.file_name();
        if &arch == "source" { continue }
        let path = branch.join(&arch);
        fs::create_dir_all(&path)?;

        Command::new("apt-ftparchive")
            .arg("packages")
            .arg(PathBuf::from(pool_base).join(&arch))
            .stderr(Stdio::inherit())
            .stdout(Stdio::piped())
            .spawn()
            .and_then(|child| {
                compress("Packages", &path, child.stdout.unwrap(), UNCOMPRESSED | GZ_COMPRESS | XZ_COMPRESS)
            })?;

        let mut release = File::create(path.join("Release"))?;
        writeln!(&mut release, "Archive: {}", config.archive)?;
        writeln!(&mut release, "Version: {}", config.version)?;
        writeln!(&mut release, "Component: main")?;
        writeln!(&mut release, "Origin: {}", config.origin)?;
        writeln!(&mut release, "Label: {}", config.label)?;
        writeln!(
            &mut release,
            "Architecture: {}",
            match arch.to_str().unwrap() {
                "binary-amd64" => "amd64",
                "binary-i386" => "i386",
                "binary-all" => "all",
                arch => panic!("unsupported architecture: {}", arch),
            }
        )?;
    }

    Ok(())
}

pub(crate) fn sources_index(dist_base: &str, pool_base: &str) -> io::Result<()> {
    info!("generating sources index");
    let path = PathBuf::from([dist_base, "/main/source/"].concat());
    fs::create_dir_all(&path)?;

    Command::new("apt-ftparchive")
        .arg("sources")
        .arg(PathBuf::from(pool_base).join("source"))
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|child| {
            compress("Sources", &path, child.stdout.unwrap(), UNCOMPRESSED | GZ_COMPRESS | XZ_COMPRESS)
        })
}

/// Generates the dists release file via `apt-ftparchive`.
pub(crate) fn dists_release(config: &Config, base: &str) -> io::Result<()> {
    info!("generating dists release files");

    let cwd = env::current_dir()?;
    env::set_current_dir(base)?;

    let release = Command::new("apt-ftparchive")
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Origin={}",
            config.origin
        ))
        .arg("-o")
        .arg(format!("APT::FTPArchive::Release::Label={}", config.label))
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Suite={}",
            config.archive
        ))
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Version={}",
            config.version
        ))
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Codename={}",
            config.archive
        ))
        .arg("-o")
        .arg("APT::FTPArchive::Release::Architectures=i386 amd64 all")
        .arg("-o")
        .arg("APT::FTPArchive::Release::Components=main")
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Description={} ({} {})",
            config.label, config.archive, config.version
        ))
        .arg("release")
        .arg(".")
        .output()
        .map(|data| data.stdout)?;

    let mut release_file = File::create("Release")?;
    release_file.write_all(&release)?;
    env::set_current_dir(cwd)
}

/// Generates the `InRelease` file from the `Release` file via `gpg --clearsign`.
pub(crate) fn gpg_in_release(email: &str, release_path: &Path, out_path: &Path) -> io::Result<()> {
    info!("generating InRelease file");
    let exit_status = Command::new("gpg")
        .args(&[
            "--clearsign",
            "--local-user",
            email,
            "--batch",
            "--yes",
            "--digest-algo",
            "sha512",
            "-o",
        ])
        .arg(out_path)
        .arg(release_path)
        .status()?;

    if exit_status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "gpg_in_release failed",
        ))
    }
}

/// Generates the `Release.gpg` file from the `Release` file via `gpg -abs`
pub(crate) fn gpg_release(email: &str, release_path: &Path, out_path: &Path) -> io::Result<()> {
    info!("generating Release.gpg file");
    let exit_status = Command::new("gpg")
        .args(&[
            "-abs",
            "--local-user",
            email,
            "--batch",
            "--yes",
            "--digest-algo",
            "sha512",
            "-o",
        ])
        .arg(out_path)
        .arg(release_path)
        .status()?;

    if exit_status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "gpg_release failed"))
    }
}

struct ContentIterator<T> {
    content: T,
}

impl<T: Iterator<Item = (PathBuf, String)>> Iterator for ContentIterator<T> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let (path, package) = self.content.next()?;
        let path = path.as_os_str().as_bytes();
        let mut serialized = Vec::new();
        serialized.extend_from_slice(if &path[..2] == b"./" {
            &path[2..]
        } else {
            path
        });
        serialized.extend_from_slice(b"  ");
        serialized.extend_from_slice(package.as_bytes());
        serialized.push(b'\n');
        Some(serialized)
    }
}

struct ContentReader<T> {
    buffer: Vec<u8>,
    data: T
}

impl<T: Iterator<Item = Vec<u8>>> io::Read for ContentReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_empty() {
            let data = match self.data.next() {
                Some(data) => data,
                None => return Ok(0)
            };

            let to_write = data.len().min(buf.len());
            buf[..to_write].copy_from_slice(&data[..to_write]);
            if to_write != data.len() {
                let leftovers = data.len() - to_write;
                if self.buffer.capacity() < leftovers {
                    let reserve = self.buffer.capacity() - leftovers;
                    self.buffer.reserve_exact(reserve);
                }

                self.buffer.truncate(leftovers);
                self.buffer.copy_from_slice(&data[to_write..]);
            }

            Ok(to_write)
        } else {
            let to_write = self.buffer.len().min(buf.len());
            buf[..to_write].copy_from_slice(&self.buffer[..to_write]);
            if to_write != self.buffer.len() {
                let leftovers = self.buffer.len() - to_write;
                if self.buffer.capacity() < leftovers {
                    let reserve = self.buffer.capacity() - leftovers;
                    self.buffer.reserve_exact(reserve);
                }

                self.buffer.truncate(leftovers);
                let temp = self.buffer[to_write..].to_owned();
                self.buffer.copy_from_slice(&temp);
            }

            Ok(to_write)
        }
    }
}

enum DecoderVariant {
    Xz,
    Gz,
}

pub(crate) fn contents(dist_base: &str, pool_base: &str) -> io::Result<()> {
    info!("generating content archives");

    let mut file_map = BTreeMap::new();
    let branch_name = "main";
    let branch: &Path = &PathBuf::from(pool_base);

    for directory in fs::read_dir(pool_base)? {
        let entry = directory?;
        let arch = entry.file_name();
        if &arch == "source" { continue }
        let path = branch.join(&arch);
        file_map.clear();

        let arch = match arch.to_str().unwrap() {
            "binary-amd64" => "amd64",
            "binary-i386" => "i386",
            "binary-all" => "all",
            arch => panic!("unsupported architecture: {}", arch),
        };

        // Collects a list of deb packages to read, and then reads them in parallel.
        let entries: Vec<io::Result<Vec<(PathBuf, String)>>> = misc::walk_debs(&path)
            .filter(|e| !e.file_type().is_dir())
            .map(|e| e.path().to_path_buf())
            .collect::<Vec<PathBuf>>()
            .into_par_iter()
            .map(|debian_entry| {
                let mut output = Vec::new();
                info!("processing contents of {:?}", debian_entry);
                let mut archive = ar::Archive::new(File::open(&debian_entry)?);

                let mut control = None;
                let mut data = None;
                let mut entry_id = 0;
                while let Some(entry_result) = archive.next_entry() {
                    if let Ok(mut entry) = entry_result {
                        match entry.header().identifier() {
                            b"data.tar.xz" => data = Some((entry_id, DecoderVariant::Xz)),
                            b"data.tar.gz" => data = Some((entry_id, DecoderVariant::Gz)),
                            b"control.tar.xz" => control = Some((entry_id, DecoderVariant::Xz)),
                            b"control.tar.gz" => control = Some((entry_id, DecoderVariant::Gz)),
                            _ => {
                                entry_id += 1;
                                continue
                            }
                        }

                        if data.is_some() && control.is_some() { break }
                    }

                    entry_id += 1;
                }

                drop(archive);

                if let (Some((data, data_codec)), Some((control, control_codec))) = (data, control) {
                    let mut package = None;
                    let mut section = None;

                    {
                        let mut archive = ar::Archive::new(File::open(&debian_entry)?);
                        let control = archive.jump_to_entry(control)?;
                        let mut reader: Box<io::Read> = match control_codec {
                            DecoderVariant::Xz => Box::new(XzDecoder::new(control)),
                            DecoderVariant::Gz => Box::new(GzDecoder::new(control)?)
                        };

                        let control_file = Path::new("./control");

                        for mut entry in tar::Archive::new(reader).entries()? {
                            let mut entry = entry?;
                            let path = entry.path()?.to_path_buf();
                            if &path == control_file {
                                for line in BufReader::new(&mut entry).lines() {
                                    let line = line?;
                                    if line.starts_with("Package:") {
                                        package = Some(line[8..].trim().to_owned());
                                    } else if line.starts_with("Section:") {
                                        section = Some(line[8..].trim().to_owned());
                                    }

                                    if package.is_some() && section.is_some() { break }
                                }
                            }
                        }
                    }

                    let package = match (package, section) {
                        (Some(ref package), Some(ref section)) if branch_name == "main" => [section, "/", package].concat(),
                        (Some(ref package), Some(ref section)) => [branch_name, "/", section, "/", package].concat(),
                        _ => unimplemented!()
                    };

                    let mut archive = ar::Archive::new(File::open(&debian_entry)?);
                    let data = archive.jump_to_entry(data)?;
                    let mut reader: Box<io::Read> = match data_codec {
                        DecoderVariant::Xz => Box::new(XzDecoder::new(data)),
                        DecoderVariant::Gz => Box::new(GzDecoder::new(data)?)
                    };

                    for entry in tar::Archive::new(reader).entries()? {
                        let entry = entry?;
                        if entry.header().entry_type().is_dir() {
                        continue 
                        }

                        let path = entry.path()?;
                        output.push((path.to_path_buf(), package.clone()));
                    }
                } else {
                    panic!("could not find data + control entries in ar archive");
                }

                Ok(output)
            }).collect();
        
        for entry in entries {
            for (path, package) in entry? {
                if let Some(duplicate) = file_map.insert(path, package) {
                    warn!("duplicate entry: {}", duplicate);
                }
            }
        }

        let mut reader = ContentReader {
            buffer: Vec::with_capacity(64 * 1024),
            data: ContentIterator {
                content: file_map.clone().into_iter()
            }
        };

        compress(&["Contents-", arch].concat(), &Path::new(dist_base), reader, GZ_COMPRESS | XZ_COMPRESS)?;
    }

    Ok(())
}