use ar;
use command::Command;
use libflate::gzip::Decoder as GzDecoder;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use tar;
use xz2::read::XzDecoder;

pub struct Archive<'a> {
    path: &'a Path,
    data: (u8, Codec),
    control: (u8, Codec)
}

impl<'a> Archive<'a> {
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
                    b"data.tar.xz" => data = Some((entry_id, Codec::Xz)),
                    b"data.tar.gz" => data = Some((entry_id, Codec::Gz)),
                    b"control.tar.xz" => control = Some((entry_id, Codec::Xz)),
                    b"control.tar.gz" => control = Some((entry_id, Codec::Gz)),
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

        Ok(Archive { path, control, data })
    }

    fn open_archive<F, T>(&self, id: u8, codec: Codec, mut func: F) -> io::Result<T>
        where F: FnMut(&mut io::Read) -> T,
    {
        let mut archive = ar::Archive::new(File::open(self.path)?);
        let inner_tar_archive = archive.jump_to_entry(id as usize)?;
        let mut reader: Box<io::Read> = match codec {
            Codec::Xz => Box::new(XzDecoder::new(inner_tar_archive)),
            Codec::Gz => Box::new(GzDecoder::new(inner_tar_archive)?)
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
            let mut control_data = BTreeMap::new();

            for mut entry in tar::Archive::new(reader).entries()? {
                let mut entry = entry?;
                let path = entry.path()?.to_path_buf();

                if path == Path::new("./control") || path == Path::new("control") {
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

    /// Validates the quality of the archive
    pub fn validate(&self) -> io::Result<()> {
        Ok(())
    }
}

// TODO: Don't rely on this
pub fn build(data: &Path, dst: &Path) -> io::Result<()> {
    Command::new("dpkg-deb").arg("-b").arg(data).arg(dst).run()
}

// TODO: Fix this, instead of using dpkg-deb
//
// pub struct Builder<'a> {
//     codec: Codec,
//     control: &'a Path,
//     data: &'a Path,
// }
//
// impl<'a> Builder<'a> {
//     /// Creates a new debian archive from the control and data directories provided.
//     ///
//     /// Note that the archives will be compressed as xz archives.
//     pub fn new(control: &'a Path, data: &'a Path) -> Self {
//         Self { codec: Codec::Xz, control, data }
//     }
//
//     /// Validates the files at the control path -- useful to prevent building an invalid package.
//     pub fn validate_control(&self) -> io::Result<()> {
//         Ok(())
//     }
//
//     /// Builds the debian archive, writing it to the given output writer.
//     pub fn build(self, output: File) -> io::Result<()> {
//         let (control, data) = rayon::join(
//             || Self::build_from(self.control, Codec::Gz),
//             || Self::build_from(self.data, self.codec)
//         );
//
//         let mut debian_binary = ::tempfile::tempfile()?;
//         debian_binary.write_all(b"2.0\n")?;
//         debian_binary.flush()?;
//         debian_binary.seek(io::SeekFrom::Start(0))?;
//
//         {
//             let mut ar = ar::Builder::new(output);
//             ar.append_file(b"debian-binary", &mut debian_binary)?;
//             ar.append_file(b"control.tar.gz", &mut control?)?;
//             ar.append_file(b"data.tar.xz", &mut data?)?;
//             ar.into_inner()?.flush()?;
//         }
//
//         Ok(())
//     }
//
//     fn append_parent_directories<W: io::Write>(tar: &mut tar::Builder<W>, appended: &mut HashSet<PathBuf>, path: &Path, mtime: u64) -> io::Result<()> {
//         if let Some(parent) = path.parent() {
//             let mut directory = PathBuf::new();
//             for component in parent.components() {
//                 if let Component::Normal(c) = component {
//                     directory.push(c);
//                     if ! appended.contains(&directory) {
//                         appended.insert(directory.clone());
//
//                         // Lintian insists on dir paths ending with /, which Rust doesn't
//                         let mut path_str = directory.to_str().unwrap().to_owned();
//                         if !path_str.ends_with('/') {
//                             path_str += "/";
//                         }
//
//                         eprintln!("D {:?}", path_str);
//                         tar.append(&cascade! {
//                             tar::Header::new_gnu();
//                             ..set_mtime(mtime);
//                             ..set_mode(0o755);
//                             ..set_path(&path_str);
//                             ..set_entry_type(tar::EntryType::Directory);
//                             ..set_size(0);
//                             ..set_cksum();
//                         }, &mut io::empty())?;
//                     }
//                 }
//             }
//         }
//
//         Ok(())
//     }
//
//     fn build_from(path: &'a Path, codec: Codec) -> io::Result<File> {
//         let mut tar = tar::Builder::new(Vec::new());
//         let mut appended = HashSet::new();
//
//         for entry in WalkDir::new(path).min_depth(1) {
//             let entry = entry.map_err(|why| io::Error::new(
//                 io::ErrorKind::Other,
//                 format!("walkdir error: {}", why)
//             ))?;
//
//             let src_path = entry.path();
//             let relative_path = src_path.strip_prefix(path).map_err(|_| io::Error::new(
//                 io::ErrorKind::Other,
//                 "failed to strip prefix"
//             ))?;
//
//             let metadata = entry.metadata()?;
//             let mtime = metadata.modified()?.duration_since(UNIX_EPOCH)
//                 .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("{}", why)))?
//                 .as_secs();
//
//             if src_path.is_file() {
//                 Self::append_parent_directories(&mut tar, &mut appended, &relative_path, mtime)?;
//                 tar.append(&cascade! {
//                     tar::Header::new_gnu();
//                     ..set_mtime(mtime);
//                     ..set_path(&relative_path);
//                     ..set_mode(metadata.permissions().mode());
//                     ..set_size(metadata.len());
//                     ..set_cksum();
//                 }, &mut File::open(src_path)?)?;
//             }
//         }
//
//         let data = tar.into_inner()?;
//         let mut out = ::tempfile::tempfile()?;
//         let mut out = match codec {
//             Codec::Xz => {
//                 let mut encoder = XzEncoder::new(out, 6);
//                 encoder.write_all(&data)?;
//                 encoder.finish()?
//             }
//             Codec::Gz => {
//                 zopfli::compress(&Options::default(), &Format::Gzip, &data, &mut out)?;
//                 out
//             }
//         };
//
//         out.flush()?;
//         out.seek(io::SeekFrom::Start(0))?;
//         Ok(out)
//     }
// }

#[derive(Copy, Clone, Debug)]
enum Codec {
    Xz,
    Gz,
}
