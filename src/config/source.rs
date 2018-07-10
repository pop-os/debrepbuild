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
    URL { url: String, checksum: String },
    Branch { url: String, branch: String }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SourceLocation {
    URL { url: String, checksum: String },
    Git { url: String, branch: Option<String> },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Source {
    pub name:      String,
    pub location:  Option<SourceLocation>,
    pub assets:    Option<Vec<SourceAsset>>,
    pub prebuild:  Option<Vec<String>>,
    pub build_on:  Option<String>,
    #[serde(default = "default_build_source")]
    pub keep_source: bool,
    pub debian:    Option<DebianPath>,
    pub depends:   Option<Vec<String>>,
    #[serde(default)]
    pub priority:  usize,
}

fn default_build_source() -> bool { true }
