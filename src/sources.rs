use std::io::{self, Read};
use std::fs::File;
use toml::{self, de};
use std::path::{Path, PathBuf};

#[derive(Debug, Fail)]
pub enum ParsingError {
    #[fail(display = "error reading '{}': {}", file, why)]
    File { file: &'static str, why: io::Error },
    #[fail(display = "failed to parse TOML syntax in {}: {}", file, why)]
    Toml { file: &'static str, why: de::Error }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub archive: String,
    pub version: String,
    pub origin: String,
    pub label: String,
    pub email: String,
    /// Packages which are already Deb packaged.
    pub direct: Vec<Direct>
}

#[derive(Debug, Deserialize)]
pub struct Direct {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub arch: String,
    pub url: String
}

impl Direct {
    pub fn destination(&self) -> PathBuf {
        PathBuf::from(["pool/main/", &self.name[0..1], "/", &self.name, "/"].concat())
    }

    pub fn file_name(&self) -> String {
        [&self.name, "_", &self.version, "-", &self.identifier, "_", &self.arch, ".deb"].concat()
    }
}

const SOURCES: &'static str = "sources.toml";

// NOTE: This was stabilized in Rust 1.26.0
fn read<P: AsRef<Path>>(path: P) -> io::Result<String> {
    File::open(path.as_ref())
        .and_then(|mut file| {
            let mut buffer = String::with_capacity(
                file.metadata().map(|x| x.len() as usize).unwrap_or(0)
            );
            file.read_to_string(&mut buffer).map(|_| buffer)
        })
}

pub fn parse() -> Result<Config, ParsingError> {
    read(SOURCES)
        .map_err(|why| ParsingError::File { file: SOURCES, why })
        .and_then(|buffer| {
            toml::from_str(&buffer)
                .map_err(|why| ParsingError::Toml { file: SOURCES, why})
        })
}
