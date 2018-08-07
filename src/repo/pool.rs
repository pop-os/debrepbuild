use std::{io, fs};
use std::path::{Path, PathBuf};
use misc;

pub const KEEP_SOURCE: u8 = 1;
pub const ARCHIVES_ONLY: u8 = 2;

pub fn mv_to_pool<P: AsRef<Path>>(path: P, archive: &str, flags: u8) -> io::Result<()> {
    pool(
        path.as_ref(),
        archive,
        flags,
        |src, dst| if flags & KEEP_SOURCE != 0 || !is_source(src) {
            fs::rename(src, dst)
        } else {
            fs::remove_file(src)
        }
    )
}

fn is_source(src: &Path) -> bool {
    let path = src.to_str().unwrap();
    path.ends_with(".dsc") || path.ends_with(".tar.gz") || path.ends_with(".tar.xz")
}

fn is_archive(src: &Path) -> bool {
    let path = src.to_str().unwrap();
    path.ends_with(".deb") || path.ends_with(".ddeb")
}

fn pool<F: Fn(&Path, &Path) -> io::Result<()>>(
    path: &Path,
    archive: &str,
    flags: u8,
    action: F
) -> io::Result<()> {
    for entry in path.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() || !(flags & ARCHIVES_ONLY != 0 && is_archive(&path)) {
            continue;
        }

        info!("migrating {} to pool", path.display());
        let filename = path.file_name().and_then(|x| x.to_str());
        let filestem = path.file_stem().and_then(|x| x.to_str());

        if let (Some(filename), Some(filestem)) = (filename, filestem) {
            let mut package = &filename[..filename.find('_').unwrap_or(0)];

            let is_source = ["dsc", "tar.xz", "tar.gz"].into_iter().any(|ext| filename.ends_with(ext));
            let destination = if is_source {
                PathBuf::from(
                    ["repo/pool/", archive, "/main/source/", &package[0..1], "/", package].concat()
                )
            } else {
                if package.ends_with("-dbgsym") {
                    package = &package[..package.len() - 7];
                }

                let arch = misc::get_arch_from_stem(filestem);

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
