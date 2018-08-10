use ar;
use libflate::gzip::Decoder as GzDecoder;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use tar;
use xz2::read::XzDecoder;

pub struct DebianArchive<'a> {
    path: &'a Path,
    data: (u8, DecoderVariant),
    control: (u8, DecoderVariant)
}

impl<'a> DebianArchive<'a> {
    /// The path given must be a valid Debian ar archive. It will be scanned to verify that the
    /// inner data.tar and control.tar entries are reachable, and records their position.
    pub fn new(path: &'a Path) -> io::Result<Self> {
        let mut archive = ar::Archive::new(File::open(path)?);

        let mut control = None;
        let mut data = None;
        let mut entry_id = 0;

        while let Some(entry_result) = archive.next_entry() {
            if let Ok(mut entry) = entry_result {
                match entry.header().identifier() {
                    b"data.tar.xz" => data = Some((entry_id, DecoderVariant::Xz)),
                    b"data.tar.gz" => data = Some((entry_id, DecoderVariant::Gz)),
                    b"control.tar.xz" => control = Some((entry_id, DecoderVariant::Xz)),
                    b"control.tar.gz" => control = Some((entry_id, DecoderVariant::Gz)),
                    _ => {
                        entry_id += 1;
                        continue
                    }
                }

                if data.is_some() && control.is_some() { break }
            }

            entry_id += 1;
        }

        let data = data.ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidData,
            format!("data archive not found in {}", path.display())
        ))?;

        let control = control.ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidData,
            format!("control archive not found in {}", path.display())
        ))?;

        Ok(DebianArchive { path, control, data })
    }

    /// Enables the caller to process entries from the inner data archive.
    pub fn data<F: FnMut(&Path)>(&self, action: F) -> io::Result<()> {
        self.inner_data(action).map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("error reading data archive within {}: {}", self.path.display(), why)
        ))
    }

    fn inner_data<F: FnMut(&Path)>(&self, mut action: F) -> io::Result<()> {
        let mut archive = ar::Archive::new(File::open(self.path)?);
        let data = archive.jump_to_entry(self.data.0 as usize)?;

        let reader: Box<io::Read> = match self.data.1 {
            DecoderVariant::Xz => Box::new(XzDecoder::new(data)),
            DecoderVariant::Gz => Box::new(GzDecoder::new(data)?)
        };

        for entry in tar::Archive::new(reader).entries()? {
            let entry = entry?;
            if entry.header().entry_type().is_dir() {
                continue
            }

            action(entry.path()?.as_ref());
        }

        Ok(())
    }

    /// Enables the caller to get the contents of the control file in the control archive as a map
    pub fn control(&self) -> io::Result<BTreeMap<String, String>> {
        self.inner_control().map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("error reading control archive within {}: {}", self.path.display(), why)
        ))
    }

    fn inner_control(&self) -> io::Result<BTreeMap<String, String>> {
        let mut archive = ar::Archive::new(File::open(self.path)?);
        let control = archive.jump_to_entry(self.control.0 as usize)?;
        let reader: Box<io::Read> = match self.control.1 {
            DecoderVariant::Xz => Box::new(XzDecoder::new(control)),
            DecoderVariant::Gz => Box::new(GzDecoder::new(control)?)
        };

        let control_file = Path::new("./control");
        let mut control_data = BTreeMap::new();

        for mut entry in tar::Archive::new(reader).entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();

            if path == control_file {
                let mut description_unset = true;
                let mut lines = BufReader::new(&mut entry).lines().peekable();
                while let Some(line) = lines.next() {
                    let line = line?;
                    if let Some(pos) = line.find(':') {
                        let (key, value) = line.split_at(pos);
                        let mut value: String = value[1..].trim().to_owned();

                        if description_unset && key == "Description" {
                            description_unset = false;
                            loop {
                                match lines.peek() {
                                    Some(next_line) => {
                                        match *next_line {
                                            Ok(ref next_line) => {
                                                if next_line.starts_with(' ') {
                                                    value.push('\n');
                                                    value.push_str(next_line);
                                                } else {
                                                    break
                                                }
                                            }
                                            Err(_) => break
                                        }
                                    }
                                    None => break
                                }

                                let _ = lines.next();
                            }
                        }

                        control_data.insert(key.to_owned(), value);
                    }
                }
            }
        }

        Ok(control_data)
    }
}

enum DecoderVariant {
    Xz,
    Gz,
}
