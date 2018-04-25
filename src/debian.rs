use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use deflate::{write::GzEncoder, Compression};
use xz2::read::XzEncoder;

use sources::Config;

pub(crate) fn generate_binary_files(config: &Config, arch: &str) -> io::Result<()> {
    eprintln!("generating binary files");
    let path = PathBuf::from(["dists/", &config.archive, "/main/binary-", arch, "/"].concat());
    fs::create_dir_all(&path)?;

    let package = Command::new("apt-ftparchive")
        .arg("packages")
        .arg("pool/main")
        .output()
        .map(|data| data.stdout)?;

    let mut gz_file = File::create(path.join("Packages.gz"))?;
    let mut compressor = GzEncoder::new(&mut gz_file, Compression::Best);
    compressor.write_all(&package)?;
    let _ = compressor.finish()?;

    let mut xz_file = File::create(path.join("Packages.xz"))?;
    let mut compressor = XzEncoder::new(package.as_slice(), 9);
    io::copy(&mut compressor, &mut xz_file)?;

    let mut release = File::create(path.join("Release"))?;
    writeln!(&mut release, "Archive: {}", config.archive)?;
    writeln!(&mut release, "Version: {}", config.version)?;
    writeln!(&mut release, "Component: main")?;
    writeln!(&mut release, "Origin: {}", config.origin)?;
    writeln!(&mut release, "Label: {}", config.label)?;
    writeln!(&mut release, "Architecture: {}", arch)?;

    Ok(())
}

pub(crate) fn generate_dists_release(config: &Config, release_path: &Path) -> io::Result<()> {
    eprintln!("generating dists release files");
    let release = Command::new("apt-ftparchive")
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Origin={}",
            config.origin
        ))
        .arg("-o")
        .arg(format!("APT::FTPArchive::Release::Label={}", config.label))
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Suite={}",
            config.archive
        ))
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Version={}",
            config.version
        ))
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Codename={}",
            config.archive
        ))
        .arg("-o")
        .arg("APT::FTPArchive::Release::Architectures=amd64")
        .arg("-o")
        .arg("APT::FTPArchive::Release::Components=main")
        .arg("-o")
        .arg(format!(
            "APT::FTPArchive::Release::Description={} ({} {})",
            config.label, config.archive, config.version
        ))
        .arg("release")
        .arg(".")
        .output()
        .map(|data| data.stdout)?;

    let mut release_file = File::create(release_path)?;
    release_file.write_all(&release)
}

pub(crate) fn gpg_in_release(email: &str, release_path: &Path, out_path: &Path) -> io::Result<()> {
    eprintln!("generating InRelease file");
    let exit_status = Command::new("gpg")
        .args(&[
            "--clearsign",
            "--local-user",
            email,
            "--batch",
            "--yes",
            "--digest-algo",
            "sha512",
            "-o",
        ])
        .arg(out_path)
        .arg(release_path)
        .status()?;

    if exit_status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "gpg_in_release failed",
        ))
    }
}

pub(crate) fn gpg_release(email: &str, release_path: &Path, out_path: &Path) -> io::Result<()> {
    eprintln!("generating Release.gpg file");
    let exit_status = Command::new("gpg")
        .args(&[
            "-abs",
            "--local-user",
            email,
            "--batch",
            "--yes",
            "--digest-algo",
            "sha512",
            "-o",
        ])
        .arg(out_path)
        .arg(release_path)
        .status()?;

    if exit_status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "gpg_release failed"))
    }
}
