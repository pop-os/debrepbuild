pub mod archive;
pub mod dist_files;
pub mod files;
pub mod misc;
pub mod package;

pub use self::archive::*;
pub use self::dist_files::*;
pub use self::files::*;
pub use self::misc::*;
pub use self::package::*;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use compress::*;

pub const DEB_SOURCE_EXTENSIONS: &[&str] = &[".tar.gz", ".tar.xz", ".dsc"];
pub const DEB_DEBUG_EXTENSION: &str = ".ddeb";
pub const DEB_EXTENSION: &str = ".deb";

pub type Arch = String;
pub type Component = String;
pub type Package = String;

pub type Control = BTreeMap<String, String>;
pub type Entries = HashMap<Arch, (HashMap<Component, Vec<PackageEntry>>, Vec<ContentsEntry>)>;

pub type ContentList = Vec<(PathBuf, String)>;
