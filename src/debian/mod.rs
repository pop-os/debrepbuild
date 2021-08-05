pub mod archive;
pub mod dist_files;
pub mod missing;
pub mod info;

pub use self::dist_files::*;
pub use self::missing::*;
pub use self::info::*;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use crate::compress::*;

pub const DEB_SOURCE_EXTENSIONS: &[&str] = &[".tar.gz", ".tar.xz", ".tar.zst", ".dsc"];
pub const DEB_DEBUG_EXTENSION: &str = ".ddeb";
pub const DEB_EXTENSION: &str = ".deb";

pub type Arch = String;
pub type Component = String;
pub type Package = String;

pub type Control = BTreeMap<String, String>;
pub type Entries = HashMap<Arch, (HashMap<Component, Vec<PackageEntry>>, Vec<ContentsEntry>)>;

pub type ContentList = Vec<(PathBuf, String)>;

pub fn gen_filename(name: &str, version: &str, arch: &str, ext: &str) -> String {
    let (name, dbg_mon, ext) = if name.ends_with("-dbg") {
        (&name[..name.len()-4], "-dbgsym", "ddeb")
    } else if name.ends_with("-dbgsym") {
        (name, "", "ddeb")
    } else if ext == "ddeb" {
        (name, "-dbgsym", "ddeb")
    } else {
        (name, "", ext)
    };

    if DEB_SOURCE_EXTENSIONS.into_iter().any(|x| &x[1..] == ext) {
        [name, dbg_mon, "_", version, ".", ext].concat()
    } else {
        [name, dbg_mon, "_", version, "_", arch, ".", ext].concat()
    }
}