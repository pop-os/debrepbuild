use std::{
    borrow::Cow,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};
use toml::{self, de};

#[derive(Debug, Fail)]
pub enum ParsingError {
    #[fail(display = "error reading '{}': {}", file, why)]
    File { file: &'static str, why:  io::Error },
    #[fail(display = "failed to parse TOML syntax in {}: {}", file, why)]
    Toml { file: &'static str, why:  de::Error },
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub archive: String,
    pub version: String,
    pub origin: String,
    pub label: String,
    pub email: String,
    /// Packages which are already Deb packaged.
    pub direct: Vec<Direct>,
    /// Projects which can be built from source
    pub source: Vec<Source>,
}

pub trait ConfigFetch {
    fn fetch<'a>(&'a self, key: &str) -> Option<Cow<'a, str>>;
}

impl ConfigFetch for Config {
    fn fetch<'a>(&'a self, key: &str) -> Option<Cow<'a, str>> {
        match key {
            "archive" => Some(Cow::Borrowed(&self.archive)),
            "version" => Some(Cow::Borrowed(&self.version)),
            "origin" => Some(Cow::Borrowed(&self.origin)),
            "label" => Some(Cow::Borrowed(&self.label)),
            "email" => Some(Cow::Borrowed(&self.email)),
            "direct" => Some(Cow::Owned(format!("{:#?}", self.direct))),
            _ => {
                if key.starts_with("direct.") {
                    let key = &key[7..];
                    let (direct_key, direct_field) =
                        key.split_at(key.find('.').unwrap_or(key.len()));

                    return match self.direct.iter().find(|d| d.name.as_str() == direct_key) {
                        Some(direct) if direct_field.len() == 1 => direct.fetch(&direct_field[1..]),
                        Some(direct) => Some(Cow::Owned(format!("{:#?}", direct))),
                        None => None
                    }
                } else if key.starts_with("source.") {
                    let key = &key[7..];
                    let (direct_key, direct_field) =
                        key.split_at(key.find('.').unwrap_or(key.len()));

                    return match self.direct.iter().find(|d| d.name.as_str() == direct_key) {
                        Some(direct) if direct_field.len() == 1 => direct.fetch(&direct_field[1..]),
                        Some(direct) => Some(Cow::Owned(format!("{:#?}", direct))),
                        None => None
                    }
                }

                None
            }
        }
    }
}

pub trait PackageEntry {
    fn destination(&self) -> PathBuf;
    fn file_name(&self) -> String;
    fn get_name(&self) -> &str;
    fn get_url(&self) -> &str;
    fn get_version(&self) -> &str;
}

#[derive(Debug, Deserialize)]
pub struct Direct {
    pub name:    String,
    pub version: String,
    pub arch:    String,
    pub url:     String,
}

impl ConfigFetch for Direct {
    fn fetch<'a>(&'a self, key: &str) -> Option<Cow<'a, str>> {
        match key {
            "name" => Some(Cow::Borrowed(&self.name)),
            "version" => Some(Cow::Borrowed(&self.version)),
            "arch" => Some(Cow::Borrowed(&self.arch)),
            "url" => Some(Cow::Borrowed(&self.url)),
            _ => None,
        }
    }
}

impl PackageEntry for Direct {
    fn get_version(&self) -> &str { &self.version }

    fn get_url(&self) -> &str { &self.url }

    fn get_name(&self) -> &str { &self.name }

    fn file_name(&self) -> String {
        [
            self.get_name(),
            "_",
            self.get_version(),
            "_",
            &self.arch,
            ".deb",
        ].concat()
    }

    fn destination(&self) -> PathBuf {
        PathBuf::from(["pool/main/", &self.name[0..1], "/", &self.name, "/"].concat())
    }
}

#[derive(Debug, Deserialize)]
pub struct Source {
    pub name: String,
    pub version: String,
    pub cvs: String,
    pub url: String,
    /// Post-obtain, pre-build commands
    pub prebuild: Vec<String>,
}

impl PackageEntry for Source {
    fn get_version(&self) -> &str { &self.version }

    fn get_url(&self) -> &str { &self.url }

    fn get_name(&self) -> &str { &self.name }

    fn file_name(&self) -> String { "".into() }

    fn destination(&self) -> PathBuf {
        PathBuf::from(["pool/main/", &self.name[0..1], "/", &self.name, "/"].concat())
    }
}

const SOURCES: &'static str = "sources.toml";

// NOTE: This was stabilized in Rust 1.26.0
fn read<P: AsRef<Path>>(path: P) -> io::Result<String> {
    File::open(path.as_ref()).and_then(|mut file| {
        let mut buffer =
            String::with_capacity(file.metadata().map(|x| x.len() as usize).unwrap_or(0));
        file.read_to_string(&mut buffer).map(|_| buffer)
    })
}

pub fn parse() -> Result<Config, ParsingError> {
    read(SOURCES)
        .map_err(|why| ParsingError::File { file: SOURCES, why })
        .and_then(|buffer| {
            toml::from_str(&buffer).map_err(|why| ParsingError::Toml { file: SOURCES, why })
        })
}
