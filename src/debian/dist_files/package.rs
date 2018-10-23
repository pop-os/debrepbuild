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
    pub fn generate_entry(mut self, origin: &str, bugs: Option<&str>) -> io::Result<Vec<u8>> {
        let mut output = Vec::with_capacity(1024);
        let control = &mut self.control;

        fn get_key(map: &mut BTreeMap<String, String>, key: &str) -> io::Result<String> {
            map.remove(key).ok_or_else(|| io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} not found in control file", key)
            ))
        }

        fn write_entry(output: &mut Vec<u8>, key: &[u8], value: &[u8]) {
            output.extend_from_slice(key);
            output.extend_from_slice(b": ");
            output.extend_from_slice(value);
            output.push(b'\n');
        }

        macro_rules! write_from_map {
            ($key:expr) => {
                 write_entry(&mut output, $key.as_bytes(), get_key(control, $key)?.as_bytes());
            }
        }

        macro_rules! optional_map {
            ($key:expr) => {
                if let Some(value) = control.remove($key) {
                    if let Some(value) = value.lines().next() {
                        write_entry(&mut output, $key.as_bytes(), value.as_bytes());
                    }
                }
            };
        }

        write_from_map!("Package");
        optional_map!("Package-Type");
        write_from_map!("Architecture");
        write_from_map!("Version");
        optional_map!("Multi-Arch");
        optional_map!("Auto-Built-Package");
        write_from_map!("Priority");
        write_from_map!("Section");
        write_entry(&mut output, b"Origin", origin.as_bytes());
        write_from_map!("Maintainer");
        write_from_map!("Installed-Size");
        optional_map!("Provides");
        optional_map!("Pre-Depends");
        optional_map!("Depends");
        optional_map!("Recommends");
        optional_map!("Suggests");
        optional_map!("Conflicts");
        optional_map!("Breaks");
        optional_map!("Replaces");
        if let Some(bugs) = bugs {
            write_entry(&mut output, b"Bugs", bugs.as_bytes());
        }
        write_entry(&mut output, b"Filename", self.filename.as_os_str().as_bytes());
        write_entry(&mut output, b"Size", self.size.to_string().as_bytes());
        write_entry(&mut output, b"MD5sum", self.md5sum.as_bytes());
        write_entry(&mut output, b"SHA1", self.sha1.as_bytes());
        write_entry(&mut output, b"SHA256", self.sha256.as_bytes());
        write_entry(&mut output, b"SHA512", self.sha512.as_bytes());
        optional_map!("Homepage");
        optional_map!("Description");
        optional_map!("License");
        optional_map!("Vendor");
        optional_map!("Build-Ids");

        Ok(output)
    }
}
