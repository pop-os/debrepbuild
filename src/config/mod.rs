use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

use crate::misc;
use toml::{self, de};

mod direct;
mod repos;
mod source;

pub use self::direct::*;
pub use self::repos::*;
pub use self::source::*;

#[derive(Debug, thiserror::Error)]
pub enum ParsingError {
    #[error("error reading {:?}: {}", file, why)]
    File { file: PathBuf, why: io::Error },
    #[error("error writing {:?}: {}", file, why)]
    FileWrite { file: PathBuf, why: io::Error },
    #[error("failed to parse TOML syntax in {:?}: {}", file, why)]
    Toml { file: PathBuf, why: de::Error },
    #[error("failed to serialize into TOML: {}", why)]
    TomlSerialize { why: toml::ser::Error },
    #[error("source URL and path defined for {}. Only one should be defined.", src)]
    SourcePathAndUrlDefined { src: String },
    #[error("neither a URL or path was defined for the source named {}", src)]
    SourceNotDefined { src: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("provided config key was not found")]
    InvalidKey,
}

/// An in-memory representation of the Debian repository's TOML spec
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Config {
    #[serde(skip)]
    pub path: PathBuf,
    pub archive: String,
    pub version: String,
    pub origin: String,
    pub label: String,
    pub email: String,
    #[serde(default = "default_architectures")]
    pub architectures: Vec<String>,
    /// Packages which are already in the deb format.
    pub direct: Option<Vec<Direct>>,
    /// Projects which can be built from source.
    pub source: Option<Vec<Source>>,
    /// Repos to pull packages from.
    pub repos: Option<Vec<Repo>>,
    #[serde(default = "default_component")]
    pub default_component: String,
    pub extra_repos: Option<Vec<String>>,
    #[serde(skip)]
    pub extra_keys: Vec<PathBuf>,
}

impl Config {
    pub fn write_to_disk(&self) -> Result<(), ParsingError> {
        toml::ser::to_vec(self)
            .map_err(|why| ParsingError::TomlSerialize { why })
            .and_then(|data| {
                File::create(&self.path)
                    .and_then(|mut file| file.write_all(&data))
                    .map_err(|why| ParsingError::FileWrite {
                        file: self.path.clone(),
                        why,
                    })
            })
    }

    pub fn direct_exists(&self, filename: &str) -> bool {
        self.direct.as_ref().map_or(false, |packages| {
            packages.iter().any(|package| {
                package.name == filename
                    || package
                        .urls
                        .iter()
                        .any(|x| x.name.as_ref().map_or(false, |x| x == filename))
            })
        })
    }

    pub fn source_exists(&self, filename: &str) -> bool {
        self.source
            .as_ref()
            .map_or(false, |x| x.iter().any(|x| x.name == filename))
    }

    pub fn package_exists(&self, filename: &str) -> bool {
        self.direct_exists(filename) || self.source_exists(filename)
    }
}

fn default_architectures() -> Vec<String> {
    vec!["amd64".into(), "i368".into()]
}
fn default_component() -> String {
    "main".into()
}

/// Methods for fetching and updating values from the in-memory representation of the TOML spec.
pub trait ConfigFetch {
    /// Fetches a given key from the TOML spec.
    fn fetch<'a>(&'a self, key: &str) -> Option<Cow<'a, str>>;

    /// Updates a given key with a specified value from the TOML spec.
    fn update(&mut self, key: &str, value: String) -> Result<(), ConfigError>;
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
                        key.split_at(key.find('.').unwrap_or_else(|| key.len()));

                    return match self
                        .direct
                        .as_ref()
                        .and_then(|direct| direct.iter().find(|d| d.name.as_str() == direct_key))
                    {
                        Some(direct) if direct_field.len() > 1 => direct.fetch(&direct_field[1..]),
                        Some(direct) => Some(Cow::Owned(format!("{:#?}", direct))),
                        None => None,
                    };
                } else if key.starts_with("source.") {
                    let key = &key[7..];
                    let (direct_key, direct_field) =
                        key.split_at(key.find('.').unwrap_or_else(|| key.len()));

                    return match self
                        .direct
                        .as_ref()
                        .and_then(|direct| direct.iter().find(|d| d.name.as_str() == direct_key))
                    {
                        Some(direct) if direct_field.len() > 1 => direct.fetch(&direct_field[1..]),
                        Some(direct) => Some(Cow::Owned(format!("{:#?}", direct))),
                        None => None,
                    };
                }

                None
            }
        }
    }

    fn update(&mut self, key: &str, value: String) -> Result<(), ConfigError> {
        match key {
            "archive" => self.archive = value,
            "version" => self.version = value,
            "origin" => self.origin = value,
            "label" => self.label = value,
            "email" => self.email = value,
            _ => {
                if key.starts_with("direct.") {
                    let key = &key[7..];
                    let (direct_key, direct_field) =
                        key.split_at(key.find('.').unwrap_or_else(|| key.len()));

                    return match self.direct.as_mut().and_then(|direct| {
                        direct.iter_mut().find(|d| d.name.as_str() == direct_key)
                    }) {
                        Some(ref mut direct) if direct_field.len() > 1 => {
                            direct.update(&direct_field[1..], value)
                        }
                        _ => Err(ConfigError::InvalidKey),
                    };
                } else if key.starts_with("source.") {
                    let key = &key[7..];
                    let (direct_key, direct_field) =
                        key.split_at(key.find('.').unwrap_or_else(|| key.len()));

                    return match self.direct.as_mut().and_then(|direct| {
                        direct.iter_mut().find(|d| d.name.as_str() == direct_key)
                    }) {
                        Some(ref mut direct) if direct_field.len() > 1 => {
                            direct.update(&direct_field[1..], value)
                        }
                        _ => Err(ConfigError::InvalidKey),
                    };
                }

                return Err(ConfigError::InvalidKey);
            }
        }

        Ok(())
    }
}

pub fn parse(path: PathBuf) -> Result<Config, ParsingError> {
    let mut config: Config = misc::read(&path)
        .map_err(|why| ParsingError::File {
            file: path.clone(),
            why,
        })
        .and_then(|buffer| {
            toml::from_slice(&buffer).map_err(|why| ParsingError::Toml {
                file: path.clone(),
                why,
            })
        })?;

    config.path = path;
    if let Ok(key_dir) = fs::read_dir("keys") {
        for key in key_dir.flat_map(|x| x.ok()) {
            let path = key.path();
            if path.extension().map_or(false, |e| e == OsStr::new("asc")) {
                if let Ok(path) = path.canonicalize() {
                    config.extra_keys.push(path);
                }
            }
        }
    }

    Ok(config)
}
