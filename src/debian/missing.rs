use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// Debian requires these files, but they're usually redundant.
pub fn create_missing_files(path: &Path) -> io::Result<()> {
    let source_dir = path.join("source");
    if !source_dir.exists() {
        fs::create_dir(&source_dir)?;
    }

    nonexistent_then_write(&source_dir.join("format"), b"3.0 (native)")?;
    nonexistent_then_write(&path.join("compat"), b"9")
}

fn nonexistent_then_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    if !path.exists() {
        write(path, contents)
    } else {
        Ok(())
    }
}

fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    File::create(path)
        .and_then(|mut file| file.write_all(contents))
        .map(|_| ())
}
