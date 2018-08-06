use config::Config;
use iter_reader::IteratorReader;
use itertools::Itertools;
use rayon::prelude::*;
use std::fs::File;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use super::*;

pub struct DistFiles<'a> {
    path: &'a Path,
    arch: &'a str,
    packages: PackageList,
    contents: ContentList
}

impl<'a> DistFiles<'a> {
    pub fn new(
        path: &'a Path,
        arch: &'a str,
        entries: Entries,
        origin: &str,
        bugs: &str,
    ) -> io::Result<Self> {
        let mut combined_capacity = 0;
        let mut contents_packages = Vec::with_capacity(entries.len());
        let mut packages = Vec::with_capacity(entries.len());

        for entry in entries {
            let entry = entry?;
            combined_capacity += entry.1.files.len();
            contents_packages.push(entry.1);
            packages.push((
                entry.0.control.get("Package").ok_or_else(|| io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} does not have Package key in control file", path.display())
                ))?.clone(),
                entry.0.generate_entry(origin, bugs)?
            ));
        }

        let mut contents = Vec::with_capacity(combined_capacity);
        
        for entry in contents_packages {
            for path in entry.files {
                contents.push((path, entry.package.clone()));
            }
        }

        contents.par_sort_unstable_by(|a, b| a.0.cmp(&b.0));
        packages.par_sort_unstable_by(|a, b| a.0.cmp(&b.0));
        Ok(DistFiles { path, arch, packages, contents })
    }

    pub fn check_for_duplicates(&self) -> io::Result<()> {
        self.inner_check_for_duplicates().map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("duplicate entry found in {}: {}", self.path.display(), why)
        ))
    }

    fn inner_check_for_duplicates(&self) -> io::Result<()> {
        let contents = &self.contents;
        contents.windows(2)
            .position(|window| window[0] == window[1])
            .map_or(Ok(()), |pos| {
                let a = &contents[pos];
                let b = &contents[pos+1];
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("{} and {} both have {}", a.1, b.1, a.0.display())
                ))
            })
    }

    pub fn compress_and_release(self, config: &Config) -> io::Result<()> {
        let contents = self.contents;
        let packages = self.packages;
        let arch = self.arch;
        let path = self.path;

        let contents_reader = IteratorReader::new(
            ContentsIterator { contents: contents.into_iter() },
            Vec::with_capacity(64 * 1024)
        );

        let packages_reader = IteratorReader::new(
            packages.into_iter().map(|(_, p)| p).intersperse(vec![b'\n']),
            Vec::with_capacity(64 * 1024)
        );

        let binary_path = &path.join("main").join(match arch {
            "amd64" => "binary-amd64",
            "i386" => "binary-i386",
            "all" => "binary-all",
            arch => panic!("unsupported architecture: {}", arch),
        });

        let (content_res, package_res) = rayon::join(
            || compress(&["Contents-", arch].concat(), path, contents_reader, GZ_COMPRESS | XZ_COMPRESS),
            || compress("Packages", binary_path, packages_reader, GZ_COMPRESS | XZ_COMPRESS)
        );

        content_res.map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("failed to generate content archive at {}: {}", path.display(), why)
        ))?;

        package_res.map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("failed to generate content archive at {}: {}", path.display(), why)
        ))?;

        inner_write_release_file(config, binary_path, arch).map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("failed to create release file for {}: {}", binary_path.display(), why)
        ))
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

fn inner_write_release_file(config: &Config, destination: &Path, arch: &str) -> io::Result<()> {
    let mut release = File::create(destination.join("Release"))?;
    writeln!(&mut release, "Archive: {}", config.archive)?;
    writeln!(&mut release, "Version: {}", config.version)?;
    writeln!(&mut release, "Component: main")?;
    writeln!(&mut release, "Origin: {}", config.origin)?;
    writeln!(&mut release, "Label: {}", config.label)?;
    writeln!(&mut release, "Architecture: {}", arch)
}