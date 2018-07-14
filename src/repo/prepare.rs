use std::{fs, io};
use std::path::Path;

pub const SHARED_ASSETS: &str = "assets/share/";
pub const PACKAGE_ASSETS: &str = "assets/packages/";

pub fn create_missing_directories() -> io::Result<()> {
    [SHARED_ASSETS, PACKAGE_ASSETS, "build", "record", "sources", "logs"].iter()
        .map(|dir| if Path::new(dir).exists() { Ok(()) } else { fs::create_dir_all(dir) })
        .collect::<io::Result<()>>()
}
