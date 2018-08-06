use std::collections::BTreeMap;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

pub struct PackageEntry {
    pub control: BTreeMap<String, String>,
    pub filename: PathBuf,
    pub size: u64,
    pub md5sum: String,
    pub sha1: String,
    pub sha256: String,
    pub sha512: String,
}

impl PackageEntry {
    pub fn generate_entry(mut self, origin: &str, bugs: &str) -> io::Result<Vec<u8>> {
        let mut output = Vec::with_capacity(1024);
        let control = &mut self.control;

        fn get_key(map: &mut BTreeMap<String, String>, key: &str) -> io::Result<String> {
            map.remove(key).ok_or_else(|| io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} not found in control file", key)
            ))
        }

        macro_rules! write_entry {
            ($key:expr, $value:expr) => {{
                output.extend_from_slice($key.as_bytes());
                output.extend_from_slice(b": ");
                output.extend_from_slice($value);
                output.push(b'\n');
            }};
        }

        macro_rules! write_from_map {
            ($key:expr) => {
                 write_entry!($key, get_key(control, $key)?.as_bytes());
            }
        }

        macro_rules! optional_map {
            ($key:expr) => {
                if let Some(value) = control.remove($key) {
                    write_entry!($key, value.as_bytes())
                }
            };
        }

        write_from_map!("Package");
        optional_map!("Package-Type");
        write_from_map!("Architecture");
        write_from_map!("Version");
        optional_map!("Auto-Built-Package");
        write_from_map!("Priority");
        write_from_map!("Section");
        write_from_map!("Maintainer");
        write_from_map!("Installed-Size");
        optional_map!("Provides");
        optional_map!("Pre-Depends");
        optional_map!("Depends");
        optional_map!("Recommends");
        optional_map!("Suggests");
        optional_map!("Conflicts");
        write_entry!("Origin", origin.as_bytes());
        write_entry!("Bugs", bugs.as_bytes());
        write_entry!("Filename", self.filename.as_os_str().as_bytes());
        write_entry!("Size", self.size.to_string().as_bytes());
        write_entry!("Md5Sum", self.md5sum.as_bytes());
        write_entry!("SHA1", self.sha1.as_bytes());
        write_entry!("SHA256", self.sha256.as_bytes());
        write_entry!("SHA512", self.sha512.as_bytes());
        optional_map!("Homepage");
        optional_map!("Description");
        optional_map!("License");
        optional_map!("Vendor");
        optional_map!("Build-Ids");

        Ok(output)
    }
}