use config::Config;
use iter_reader::IteratorReader;
use itertools::Itertools;
use rayon::prelude::*;
use rayon::*;
use std::fs::File;
use std::io::{self, Error, ErrorKind, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use super::*;

pub struct DistFiles<'a> {
    path: &'a Path,
    entries: Entries
}

impl<'a> DistFiles<'a> {
    pub fn new(path: &'a Path, entries: Entries) -> Self {
        DistFiles { path, entries }
    }

    // pub fn check_for_duplicates(&self) {
    //     if let Err(why) = self.inner_check_for_duplicates() {
    //         warn!("duplicate entry found in {}: {}", self.path.display(), why);
    //     }
    // }
    //
    // fn inner_check_for_duplicates(&self) -> io::Result<()> {
    //     let contents = &self.contents;
    //     contents.windows(2)
    //         .position(|window| window[0] == window[1])
    //         .map_or(Ok(()), |pos| {
    //             let a = &contents[pos];
    //             let b = &contents[pos+1];
    //             Err(io::Error::new(
    //                 io::ErrorKind::Other,
    //                 format!("{} and {} both have {}", a.1, b.1, a.0.display())
    //             ))
    //         })
    // }

    pub fn compress_and_release(self, config: &Config, origin: &str, bugs: Option<&str>) -> io::Result<()> {
        let entries = self.entries;
        let path = self.path;

        entries.into_par_iter().map(|(arch, (packages, contents))| {
            let arch: &str = &arch;
            let (contents_res, packages_res) = rayon::join(
                // Generate and compress the Contents archive for each architecture in parallel
                || {
                    // TODO: unnecessary heap allocation
                    let mut temp_contents = Vec::new();

                    for entry in contents {
                        for path in entry.files {
                            temp_contents.push((path, entry.package.clone()));
                        }
                    }

                    let contents_reader = IteratorReader::new(
                        ContentsIterator {
                            contents: temp_contents.into_iter()
                        },
                        Vec::with_capacity(64 * 1024)
                    );

                    compress(&["Contents-", &arch].concat(), path, contents_reader, UNCOMPRESSED | GZ_COMPRESS | XZ_COMPRESS)
                },
                // Generate & compress each Packages archive for each architecture & component in parallel.
                || {
                    let arch_dir = match arch {
                        "amd64" => "binary-amd64",
                        "i386" => "binary-i386",
                        "all" => "binary-all",
                        arch => panic!("unsupported architecture: {}", arch),
                    };

                    packages.into_par_iter().map(|(component, packages)| {
                        let binary_path = &path.join(&component).join(arch_dir);

                        let mut generated_packages = Vec::new();
                        for package in packages {
                            generated_packages.push((
                                package.control.get("Package").ok_or_else(|| io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("{} does not have Package key in control file", path.display())
                                ))?.clone(),
                                package.generate_entry(origin, bugs)?
                            ))
                        }

                        let packages_reader = IteratorReader::new(
                            generated_packages.into_iter().map(|(_, p)| p).intersperse(vec![b'\n']),
                            Vec::with_capacity(64 * 1024)
                        );

                        compress("Packages", binary_path, packages_reader, UNCOMPRESSED| GZ_COMPRESS | XZ_COMPRESS)
                            .map_err(|why| io::Error::new(
                                io::ErrorKind::Other,
                                format!("failed to generate content archive at {}: {}", path.display(), why)
                            ))?;

                        inner_write_release_file(config, binary_path, arch_dir, &component).map_err(|why| io::Error::new(
                            io::ErrorKind::Other,
                            format!("failed to create release file for {}: {}", binary_path.display(), why)
                        ))
                    }).collect::<io::Result<()>>()
                }
            );

            contents_res.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to generate content archive at {}: {}", path.display(), why)
            ))?;

            packages_res
        }).collect::<io::Result<()>>()
    }
}

pub struct ContentsIterator<T> {
    pub contents: T,
}

impl<T: Iterator<Item = (PathBuf, String)>> Iterator for ContentsIterator<T> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let (path, package) = self.contents.next()?;
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

pub struct ContentsEntry {
    pub package: String,
    pub files: Vec<PathBuf>
}

fn inner_write_release_file(config: &Config, destination: &Path, arch: &str, component: &str) -> io::Result<()> {
    let mut release = File::create(destination.join("Release"))?;
    writeln!(&mut release, "Archive: {}", config.archive)?;
    writeln!(&mut release, "Version: {}", config.version)?;
    writeln!(&mut release, "Component: {}", component)?;
    writeln!(&mut release, "Origin: {}", config.origin)?;
    writeln!(&mut release, "Label: {}", config.label)?;
    writeln!(&mut release, "Architecture: {}", arch)
}
