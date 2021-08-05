use crate::debian::DEB_SOURCE_EXTENSIONS;
use std::path::PathBuf;
use std::borrow::Cow;
use std::io;
use crate::url::UrlTokenizer;
use super::{ConfigError, ConfigFetch};
use crate::misc;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Update {
    pub source:     String,
    pub url:        String,
    pub after:      String,
    pub before:     String,
    pub contains:   Option<String>,
    pub build_from: Option<Vec<String>>,
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
                        "gz" | "xz" | "zst" => if "tar" == &file[ext_pos - 4..ext_pos - 1] {
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
                    "tar.gz" | "tar.xz" | "tar.zst" | "dsc" => ["/", component, "/source/"].concat(),
                    _ => ["/", component, "/binary-", arch, "/"].concat()
                };

                if extension == "deb" {
                    let base = format!("assets/replace/{}{}/{}/", suite, dst, name);
                    let files = PathBuf::from([&base, "files"].concat());
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