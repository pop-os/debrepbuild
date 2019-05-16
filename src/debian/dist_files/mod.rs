mod package;

use config::Config;
use iter_reader::IteratorReader;
use itertools::Itertools;
use rayon;
use rayon::prelude::*;
use std::fs::File;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
pub use self::package::*;
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
    //     let contents = &self.contents;
    //     for (arch, (packages, contents)) in &self.entries {
    //
    //     }
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

        // Processes each architecture in parallel, including the contents archives for each arch.
        entries.into_par_iter().map(|(arch, (packages, contents))| {
            let arch: &str = &arch;
            let (contents_res, packages_res) = rayon::join(
                // Generate and compress the Contents archive for each architecture in parallel.
                // Contents are processed in a per-architecture manner, rather than per-component.
                || {
                    // Sort the files beforehand, so files are easy to track down.
                    // This will require that we generate the contents archive in advance, sadly.
                    let mut contents = ContentsIterator::new(contents).collect::<Vec<Vec<u8>>>();
                    contents.par_sort_unstable_by(|a, b| a.cmp(&b));

                    let contents_reader = IteratorReader::new(
                        contents.into_iter(),
                        Vec::with_capacity(64 * 1024)
                    );

                    // Similar to the Packages archives, we also need an uncompressed variant of
                    // the compressed archives to satisfy APT's detection capabilities.
                    compress(&["Contents-", &arch].concat(), path, contents_reader, UNCOMPRESSED | GZ_COMPRESS | XZ_COMPRESS)
                },
                // Generate & compress each Packages archive for each architecture & component in parallel.
                // Packages archives are processed in a per-architecture, per-component manner.
                || {
                    let arch_dir = match arch {
                        "amd64" => "binary-amd64",
                        "arm64" => "binary-arm64",
                        "armel" => "binary-armel",
                        "armhf" => "binary-armhf",
                        "i386" => "binary-i386",
                        "mips" => "binary-mips",
                        "mipsel" => "binary-mipsel",
                        "mips64el" => "binary-mips64el",
                        "ppc64el" => "binary-ppc64el",
                        "s390x" => "binary-s390x",
                        "all" => "binary-all",
                        arch => panic!("unsupported architecture: {}", arch),
                    };

                    // Processes the packages of each component in parallel, for this architecture.
                    packages.into_par_iter().map(|(component, mut packages)| {
                        // Construct the path where the Packages archives will be written.
                        let binary_path = &path.join(&component).join(arch_dir);

                        // Sort the packages that were collected before we generate them for writing.
                        packages.par_sort_unstable_by(|a, b| a.filename.cmp(&b.filename));

                        // Generate the packages content in advance so that we can handle the errors.
                        let mut generated_packages = Vec::new();
                        for package in packages {
                            generated_packages.push(package.generate_entry(origin, bugs)?)
                        }

                        // This iterator will be supplied to our compressor, writing the final
                        // output with an empty newline between each entry.
                        let packages_reader = IteratorReader::new(
                            generated_packages.into_iter().map(|p| p).intersperse(vec![b'\n']),
                            Vec::with_capacity(64 * 1024)
                        );

                        // Although we will generate a compressed GZ and XZ archive for our
                        // repository, APT still requires that we also write an uncompressed variant.
                        compress("Packages", binary_path, packages_reader, UNCOMPRESSED | GZ_COMPRESS | XZ_COMPRESS)
                            .map_err(|why| io::Error::new(
                                io::ErrorKind::Other,
                                format!("failed to generate content archive at {}: {}", path.display(), why)
                            ))?;

                        // A release file also needs to be stored in the same location, after the
                        // archives have been written. This contains the checksums for each file.
                        inner_write_release_file(config, binary_path, arch_dir, &component).map_err(|why| io::Error::new(
                            io::ErrorKind::Other,
                            format!("failed to create release file for {}: {}", binary_path.display(), why)
                        ))
                    }).collect::<io::Result<()>>()
                }
            );

            // Check the results to see if we passed.
            contents_res.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to generate content archive at {}: {}", path.display(), why)
            ))?;

            packages_res
        }).collect::<io::Result<()>>()
    }
}

/// Efficiently generate each line of the Contents file, in style.
pub struct ContentsIterator {
    contents: Vec<ContentsEntry>,
    buffer: Vec<u8>,
    package: usize,
    file: usize
}

impl ContentsIterator {
    pub fn new(contents: Vec<ContentsEntry>) -> Self {
        ContentsIterator { contents, buffer: Vec::with_capacity(512), package: 0, file: 0 }
    }
}

impl Iterator for ContentsIterator {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let contents_entry = self.contents.get(self.package)?;
            match contents_entry.files.get(self.file) {
                Some(path) => {
                    self.file += 1;
                    let path = path.as_os_str().as_bytes();
                    self.buffer.extend_from_slice(if &path[..2] == b"./" { &path[2..] } else { path });
                    self.buffer.extend_from_slice(b"  ");
                    self.buffer.extend_from_slice(contents_entry.package.as_bytes());
                    self.buffer.push(b'\n');
                    let mut serialized = self.buffer.clone();
                    serialized.shrink_to_fit();
                    self.buffer.clear();
                    return Some(serialized);
                }
                None => {
                    self.file = 0;
                    self.package += 1;
                }
            }
        }
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
