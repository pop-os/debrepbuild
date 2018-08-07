use std::io::{self, Write};
use std::fs::{self, File};
use std::path::Path;

pub fn create_missing_files(path: &Path) -> io::Result<()> {
    let source_dir = path.join("source");
    if !source_dir.exists() {
        fs::create_dir(&source_dir)?;
    }

    let source_format = source_dir.join("format");
    if !source_format.exists() {
        write(&source_format, b"3.0 (native)")?;
    }

    let compat = path.join("compat");
    if !compat.exists() {
        write(&compat, b"9")?;
    }

    Ok(())
}

fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    File::create(path)
        .and_then(|mut file| file.write_all(contents))
        .map(|_| ())
}
