pub mod archive;
pub mod dist_files;
pub mod package;

pub use self::archive::*;
pub use self::dist_files::*;
pub use self::package::*;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::io;
use rayon;
use compress::*;

pub type Control = BTreeMap<String, String>;
pub type Entries = Vec<io::Result<(PackageEntry, ContentsEntry)>>;
pub type PackageList = Vec<(String, Vec<u8>)>;
pub type ContentList = Vec<(PathBuf, String)>;