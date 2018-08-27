use debian::DEB_SOURCE_EXTENSIONS;
use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use toml::{self, de};
use misc;
use url::UrlTokenizer;

mod source;

pub use self::source::*;

/// Currently hard-coded to search for `sources.toml` in the current working directory.
const SOURCES: &str = "sources.toml";

#[derive(Debug, Fail)]
pub enum ParsingError {
    #[fail(display = "error reading '{}': {}", file, why)]
    File { file: &'static str, why:  io::Error },
    #[fail(display = "error writing '{}': {}", file, why)]
    FileWrite { file: &'static str, why:  io::Error },
    #[fail(display = "failed to parse TOML syntax in {}: {}", file, why)]
    Toml { file: &'static str, why:  de::Error },
    #[fail(display = "failed to serialize into TOML: {}", why)]
    TomlSerialize { why: toml::ser::Error },
    #[fail(display = "source URL and path defined for {}. Only one should be defined.", source)]
    SourcePathAndUrlDefined { source: String },
    #[fail(display = "neither a URL or path was defined for the source named {}", source)]
    SourceNotDefined { source: String }
}

#[derive(Debug, Fail)]
pub enum ConfigError {
    #[fail(display = "provided config key was not found")]
    InvalidKey,
}

/// An in-memory representation of the Debian repository's TOML spec
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Config {
    pub archive: String,
    pub version: String,
    pub origin: String,
    pub label: String,
    pub email: String,
    /// Packages which are already Deb packaged.
    pub direct: Option<Vec<Direct>>,
    /// Projects which can be built from source
    pub source: Option<Vec<Source>>,
    #[serde(default = "default_component")]
    pub default_component: String,
}

impl Config {
    pub fn write_to_disk(&self) -> Result<(), ParsingError> {
        toml::ser::to_vec(self)
            .map_err(|why| ParsingError::TomlSerialize { why })
            .and_then(|data| {
                File::create(SOURCES)
                    .and_then(|mut file| file.write_all(&data))
                    .map_err(|why| ParsingError::FileWrite { file: SOURCES, why })
            })
    }

    pub fn direct_exists(&self, filename: &str) -> bool {
        self.direct.as_ref()
            .map_or(false, |packages| {
                packages.iter().any(|package| {
                    package.name == filename
                        || package.urls.iter()
                            .any(|x| x.name.as_ref().map_or(false, |x| x == filename))
                })
            })
    }

    pub fn source_exists(&self, filename: &str) -> bool {
        self.source.as_ref()
            .map_or(false, |x| x.iter().any(|x| x.name == filename))
    }

    pub fn package_exists(&self, filename: &str) -> bool {
        self.direct_exists(filename) || self.source_exists(filename)
    }
}

fn default_component() -> String { "main".into() }

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

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Update {
    pub source:     String,
    pub url:        String,
    pub after:      String,
    pub before:     String,
    pub contains:   Option<String>,
    pub build_from: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct DirectPath {
    pub checksum: Option<String>,
    pub arch:     Option<String>,
    pub name:     Option<String>,
    pub url:      String,
}

/// A Debian package which already exists and may be downloaded directly.
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Direct {
    pub name:      String,
    pub version:   String,
    pub urls:      Vec<DirectPath>,
    pub checksum:  Option<String>,
    pub update:    Option<Update>,
}

/// Stores where the file can be downloaded, and where that file should be stored.
#[derive(Debug)]
pub struct BinaryDestinations {
    /// Where the files to repackage with exist, and where the original package is stored.
    pub assets: Option<(PathBuf, PathBuf)>,
    /// Where the repackaged file will be stored.
    pub pool: PathBuf,
    /// Where the file can be obtained
    pub url: String,
}

impl ConfigFetch for Direct {
    fn fetch<'a>(&'a self, key: &str) -> Option<Cow<'a, str>> {
        match key {
            "name" => Some(Cow::Borrowed(&self.name)),
            "version" => Some(Cow::Borrowed(&self.version)),
            "urls" => Some(Cow::Owned(format!("{:#?}", self.urls))),
            _ => None,
        }
    }

    fn update(&mut self, key: &str, value: String) -> Result<(), ConfigError> {
        match key {
            "name" => self.name = value,
            "version" => self.version = value,
            _ => return Err(ConfigError::InvalidKey),
        }

        Ok(())
    }
}

impl Direct {
    pub fn get_destinations(&self, suite: &str, component: &str) -> io::Result<Vec<BinaryDestinations>> {
        let mut output = Vec::new();

        fn gen_filename(name: &str, version: &str, arch: &str, ext: &str) -> String {
            if DEB_SOURCE_EXTENSIONS.into_iter().any(|x| &x[1..] == ext) {
                [name, if ext == "ddeb" { "-dbgsym_" } else { "_" }, version, ".", ext].concat()
            } else {
                [name, if ext == "ddeb" { "-dbgsym_" } else { "_" }, version, "_", arch, ".", ext].concat()
            }
        }

        for file_item in &self.urls {
            let name: &str = file_item.name.as_ref().map_or(&self.name, |x| &x);
            let url = UrlTokenizer::finalize(&file_item.url, name, &self.version)
                .map_err(|text|
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("unsupported variable: {}", text)
                    )
                )?;

            let mut assets = None;

            let pool = {
                let file = &url[url.rfind('/').unwrap_or(0) + 1..];

                let ext_pos = {
                    let mut ext_pos = file.rfind('.').unwrap_or_else(|| file.len()) + 1;
                    match &file[ext_pos..] {
                        "gz" | "xz" => if "tar" == &file[ext_pos - 4..ext_pos - 1] {
                            ext_pos -= 4;
                        }
                        _ => ()
                    }
                    ext_pos
                };

                let extension = &file[ext_pos..];
                let arch = match file_item.arch.as_ref() {
                    Some(ref arch) => arch.as_str(),
                    None => misc::get_arch_from_stem(&file[..ext_pos - 1]),
                };

                let filename = gen_filename(name, &self.version, arch, extension);
                let dst = match extension {
                    "tar.gz" | "tar.xz" | "dsc" => ["/", component, "/source/"].concat(),
                    _ => ["/", component, "/binary-", arch, "/"].concat()
                };

                if extension == "deb" {
                    let base = format!("assets/replace/{}{}/{}/", suite, dst, name);
                    let files = PathBuf::from([&base, "files"].concat());
                    debug!("{:?} exists?", files);
                    if files.exists() {
                        let replace = PathBuf::from([base.as_str(), filename.as_str()].concat());
                        debug!("setting asset target to {:?}", replace);
                        assets = Some((files, replace));
                    }
                }


                PathBuf::from(["repo/pool/", suite, &dst, &name[0..1], "/", name, "/", &filename].concat())
            };

            output.push(BinaryDestinations { assets, pool, url });
        }

        Ok(output)
    }
}

pub fn parse() -> Result<Config, ParsingError> {
    misc::read(SOURCES)
        .map_err(|why| ParsingError::File { file: SOURCES, why })
        .and_then(|buffer| {
            toml::from_slice(&buffer).map_err(|why| ParsingError::Toml { file: SOURCES, why })
        })
}
