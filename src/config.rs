use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use toml::{self, de};
use misc;

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
#[derive(Debug, Deserialize, Serialize)]
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

/// Methods shared between `Direct` and `Source` structures.
pub trait PackageEntry {
    fn destination(&self) -> PathBuf;
    fn file_name(&self) -> String;
    fn get_name(&self) -> &str;
    fn get_url(&self) -> &str;
    fn get_version(&self) -> &str;
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Update {
    pub source:     String,
    pub url:        String,
    pub after:      String,
    pub before:     String,
    pub contains:   Option<String>,
    pub build_from: Option<Vec<String>>,
}

/// A Debian package which already exists and may be downloaded directly.
#[derive(Debug, Deserialize, Serialize)]
pub struct Direct {
    pub name:      String,
    pub version:   String,
    pub arch:      String,
    pub url:       String,
    pub checksum:  Option<String>,
    pub update:    Option<Update>,
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

    fn update(&mut self, key: &str, value: String) -> Result<(), ConfigError> {
        match key {
            "name" => self.name = value,
            "version" => self.version = value,
            "arch" => self.arch = value,
            "url" => self.url = value,
            _ => return Err(ConfigError::InvalidKey),
        }

        Ok(())
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
        PathBuf::from(
            [
                "pool/main/binary-",
                &self.arch,
                "/",
                &self.name[0..1],
                "/",
                &self.name,
                "/",
            ].concat(),
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceMember {
    pub name:      String,
    pub directory: PathBuf,
    pub build_on:  Option<String>,
    pub debian:    Option<DebianPath>,
    #[serde(default)]
    pub priority:  usize,
}

// Files that we want to cache and re-use between runs. These files will be symlinked.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceArtifact {
    pub src: String,
    pub dst: PathBuf,
}

/// In the event that the source does not have a debian directory, we may designate the location of
/// the debian files here.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DebianPath {
    Path { path: PathBuf },
    URL { url: String },
    Branch { branch: String }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SourceLocation {
    Path { path: PathBuf },
    URL { url: String, checksum: String },
    Git { url: String },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Source {
    pub name:      String,
    pub location:  SourceLocation,
    pub members:   Option<Vec<SourceMember>>,
    pub artifacts: Option<Vec<SourceArtifact>>,
    pub prebuild:  Option<Vec<String>>,
    pub build_on:  Option<String>,
    pub debian:    Option<DebianPath>,
    #[serde(default)]
    pub priority:  usize,
}

pub fn parse() -> Result<Config, ParsingError> {
    misc::read(SOURCES)
        .map_err(|why| ParsingError::File { file: SOURCES, why })
        .and_then(|buffer| {
            toml::from_slice(&buffer).map_err(|why| ParsingError::Toml { file: SOURCES, why })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    const EXAMPLE: &str = r#"archive = "bionic"
    version = "18.04"
    origin = "system76-example"
    label = "Pop!_OS Example"
    email = "michael@system76.com"

    [[direct]]
    name = "atom-editor"
    version = "1.26.0"
    arch = "amd64"
    url = "https://atom-installer.github.com/v1.26.0/atom-amd64.deb"

    [[direct]]
    name = "code"
    version = "1.22.2-1523551015"
    arch = "amd64"
    url = "https://az764295.vo.msecnd.net/stable/3aeede733d9a3098f7b4bdc1f66b63b0f48c1ef9/code_1.22.2-1523551015_amd64.deb""#;

    #[test]
    fn fetch() {
        let config = toml::from_str::<Config>(EXAMPLE).unwrap();

        assert_eq!(config.fetch("version").as_ref().unwrap(), "18.04");
        assert_eq!(
            config.fetch("direct.atom-editor.version").as_ref().unwrap(),
            "1.26.0"
        );
        assert_eq!(
            config.fetch("direct.code.version").as_ref().unwrap(),
            "1.22.2-1523551015"
        );
    }

    #[test]
    fn update() {
        let mut config = toml::from_str::<Config>(EXAMPLE).unwrap();

        assert_eq!(config.fetch("archive").as_ref().unwrap(), "bionic");
        config.update("archive", "redox".into()).unwrap();
        assert_eq!(config.fetch("archive").as_ref().unwrap(), "redox");

        assert_eq!(
            config.fetch("direct.atom-editor.version").as_ref().unwrap(),
            "1.26.0"
        );
        config
            .update("direct.atom-editor.version", "1.27.0".into())
            .unwrap();
        assert_eq!(
            config.fetch("direct.atom-editor.version").as_ref().unwrap(),
            "1.27.0"
        );
    }
}
