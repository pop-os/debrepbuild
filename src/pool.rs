use std::{io, fs};
use std::path::{Path, PathBuf};

pub fn mv_to_pool<P: AsRef<Path>>(path: P, archive: &str) -> io::Result<()> {
    pool(path.as_ref(), archive, |src, dst| fs::rename(src, dst))
}

pub fn cp_to_pool<P: AsRef<Path>>(path: P, archive: &str) -> io::Result<()> {
    pool(path.as_ref(), archive, |src, dst| fs::copy(src, dst).map(|_| ()))
}

fn pool<F: Fn(&Path, &Path) -> io::Result<()>>(path: &Path, archive: &str, action: F) -> io::Result<()> {
    for entry in path.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        let filename = path.file_name().and_then(|x| x.to_str());
        let filestem = path.file_stem().and_then(|x| x.to_str());

        if let (Some(filename), Some(filestem)) = (filename, filestem) {
            let mut package = &filename[..filename.find('_').unwrap_or(0)];

            let is_source = ["dsc", "tar.xz"].into_iter().any(|ext| filename.ends_with(ext));
            let destination = if is_source {
                PathBuf::from(
                    ["repo/pool/", archive, "/main/source/", &package[0..1], "/", package].concat()
                )
            } else {
                if package.ends_with("-dbgsym") {
                    package = &package[..package.len() - 7];
                }

                let mut arch = &filestem[filestem.rfind('_').unwrap_or(0) + 1..];
                arch = arch.find('-').map_or(arch, |pos| &arch[..pos]);

                PathBuf::from(
                    ["repo/pool/", archive, "/main/binary-", arch, "/", &package[0..1], "/", package].concat(),
                )
            };

            info!("creating in pool: {:?}", destination);
            fs::create_dir_all(&destination)?;
            action(&path, &destination.join(filename))?;
        }
    }

    Ok(())
}
