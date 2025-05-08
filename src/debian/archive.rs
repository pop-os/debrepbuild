use crate::command::Command;
use std::{io, path::Path};

// TODO: Don't rely on this
pub fn build(data: &Path, dst: &Path) -> io::Result<()> {
    Command::new("dpkg-deb").arg("-b").arg(data).arg(dst).run()
}
