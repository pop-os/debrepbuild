use crate::command::Command;
use std::path::Path;
use std::{io, fs};

pub fn rsync(src: &Path, dst: &Path) -> io::Result<()> {
    info!("rsyncing {} to {}", src.display(), dst.display());

    if src.is_dir() && ! dst.exists() {
        fs::create_dir_all(dst).map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("failed to create destination directory at {:?} for rsync: {}", dst, why)
        ))?;
    }

    Command::new("rsync").args(&["--ignore-existing", "-avz"]).arg(src).arg(dst).run()
}
