use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Files that we want to cache and re-use between runs. These files will be symlinked.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceAsset {
    pub src: String,
    pub dst: PathBuf,
}

/// In the event that the source does not have a debian directory, we may designate the location of
/// the debian files here.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DebianPath {
    /// Fetches the debian directory from a separate URL.
    URL { url: String, checksum: String },
    /// Fetches the debian directory from a separate branch.
    Branch { url: String, branch: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SourceLocation {
    /// Fetch the source from a remote tarball.
    URL { url: String, checksum: String },
    /// Fetch the source by the git repository where it can be reached.
    Git {
        git: String,
        branch: Option<String>,
        commit: Option<String>,
    },
    /// Fetch the source by an existing remote debian `.dsc` file.
    Dsc { dsc: String },
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Source {
    pub name: String,
    pub version: Option<String>,
    pub location: Option<SourceLocation>,
    pub assets: Option<Vec<SourceAsset>>,
    pub starting_build: Option<Vec<String>>,
    pub prebuild: Option<Vec<String>>,
    pub build_on: Option<String>,
    pub repos: Option<Vec<String>>,
    #[serde(default = "default_build_source")]
    pub keep_source: bool,
    pub debian: Option<DebianPath>,
    pub depends: Option<Vec<String>>,
    #[serde(default = "default_retain")]
    pub retain: usize,
    #[serde(default = "default_requires_extract")]
    pub extract: bool,
}

fn default_build_source() -> bool {
    true
}
fn default_retain() -> usize {
    3
}
fn default_requires_extract() -> bool {
    true
}
