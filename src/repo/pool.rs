use std::{io, fs};
use std::path::{Path, PathBuf};
use misc;

pub const KEEP_SOURCE: u8 = 1;
pub const ARCHIVES_ONLY: u8 = 2;

pub fn mv_to_pool<P: AsRef<Path>>(path: P, suite: &str, branch: &str, flags: u8, filter: Option<&str>) -> io::Result<()> {
    info!("moving items in {} to pool at {}/{}", path.as_ref().display(), suite, branch);
    pool(
        path.as_ref(),
        suite,
        branch,
        flags,
        |src, dst| if flags & KEEP_SOURCE != 0 || !is_source(src) {
            fs::rename(src, dst)
        } else {
            fs::remove_file(src)
        },
        filter
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
    suite: &str,
    branch: &str,
    flags: u8,
    action: F,
    filter: Option<&str>,
) -> io::Result<()> {
    for entry in path.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        eprintln!("found {}", path.display());
        if path.is_dir() || (flags & ARCHIVES_ONLY != 0 && !is_archive(&path)) {
            continue;
        }
        let filename = path.file_name().and_then(|x| x.to_str());
        let filestem = path.file_stem().and_then(|x| x.to_str());

        if let (Some(filename), Some(filestem)) = (filename, filestem) {
            if let Some(name) = filter {
                if !(filename.starts_with(&[name, "_"].concat())
                    || filename.starts_with(&[name, "-dbgsym_"].concat()))
                {
                    continue
                }
            }

            info!("migrating {} to pool", path.display());
            let mut package = &filename[..filename.find('_').unwrap_or(0)];

            let is_source = ["dsc", "tar.xz", "tar.gz"].into_iter().any(|ext| filename.ends_with(ext));
            let destination = if is_source {
                PathBuf::from(
                    ["repo/pool/", suite, "/", branch, "/source/", &package[0..1], "/", package].concat()
                )
            } else {
                if package.ends_with("-dbgsym") {
                    package = &package[..package.len() - 7];
                }

                let arch = misc::get_arch_from_stem(filestem);

                PathBuf::from(
                    ["repo/pool/", suite, "/", branch, "/binary-", arch, "/", &package[0..1], "/", package].concat(),
                )
            };

            info!("creating in pool: {:?}", destination);
            fs::create_dir_all(&destination)?;
            action(&path, &destination.join(filename))?;
        }
    }

    Ok(())
}
