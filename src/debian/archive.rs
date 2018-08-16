use ar;
use libflate::gzip::Decoder as GzDecoder;
use std::collections::BTreeMap;
use std::fs::{self, File};
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

    fn open_archive<F, T>(&self, id: u8, codec: DecoderVariant, mut func: F) -> io::Result<T>
        where F: FnMut(&mut dyn io::Read) -> T,
    {
        let mut archive = ar::Archive::new(File::open(self.path)?);
        let control = archive.jump_to_entry(id as usize)?;
        let mut reader: Box<io::Read> = match codec {
            DecoderVariant::Xz => Box::new(XzDecoder::new(control)),
            DecoderVariant::Gz => Box::new(GzDecoder::new(control)?)
        };

        Ok(func(reader.as_mut()))
    }

    /// Unpacks the inner data archive to the given path.
    pub fn extract_data<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        if !path.exists() {
            fs::create_dir_all(path)?;
        }

        let (id, codec) = (self.data.0, self.data.1);
        self.open_archive(id, codec, |reader| tar::Archive::new(reader).unpack(path))?
    }

    /// Enables the caller to process entries from the inner data archive.
    pub fn data<F: FnMut(&Path)>(&self, action: F) -> io::Result<()> {
        self.inner_data(action).map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("error reading data archive within {}: {}", self.path.display(), why)
        ))
    }

    fn inner_data<F: FnMut(&Path)>(&self, mut action: F) -> io::Result<()> {
        let (id, codec) = (self.data.0, self.data.1);
        self.open_archive(id, codec, |reader| {
            for entry in tar::Archive::new(reader).entries()? {
                let entry = entry?;
                if entry.header().entry_type().is_dir() {
                    continue
                }

                action(entry.path()?.as_ref());
            }

            Ok(())
        })?
    }

    /// Unpacks the inner control archive to the given path.
    pub fn extract_control<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        if !path.exists() {
            fs::create_dir_all(path)?;
        }

        let (id, codec) = (self.control.0, self.control.1);
        self.open_archive(id, codec, |reader| tar::Archive::new(reader).unpack(path))?
    }

    /// Enables the caller to get the contents of the control file in the control archive as a map
    pub fn control(&self) -> io::Result<BTreeMap<String, String>> {
        self.inner_control().map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("error reading control archive within {}: {}", self.path.display(), why)
        ))
    }

    fn inner_control(&self) -> io::Result<BTreeMap<String, String>> {
        let (id, codec) = (self.control.0, self.control.1);
        self.open_archive(id, codec, |reader| {
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
        })?
    }
}

#[derive(Copy, Clone, Debug)]
enum DecoderVariant {
    Xz,
    Gz,
}
