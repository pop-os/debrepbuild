use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use deflate::write::GzEncoder;
use deflate::Compression;
use xz2::read::XzEncoder;

use config::Config;

/// Generates the binary files from Debian packages that exist within the pool, using
/// `apt-ftparchive`
pub(crate) fn generate_binary_files(config: &Config, dist_base: &str, pool_base: &str) -> io::Result<()> {
    info!("generating binary files");
    let branch = PathBuf::from([dist_base, "/main/"].concat());

    for directory in fs::read_dir(pool_base)? {
        let entry = directory?;
        let arch = entry.file_name();
        if &arch == "source" { continue }
        let path = branch.join(&arch);
        fs::create_dir_all(&path)?;

        let package = Command::new("apt-ftparchive")
            .arg("packages")
            .arg(PathBuf::from(pool_base).join(&arch))
            .output()
            .map(|data| data.stdout)?;

        compress("Packages", &path, &package)?;

        let mut release = File::create(path.join("Release"))?;
        writeln!(&mut release, "Archive: {}", config.archive)?;
        writeln!(&mut release, "Version: {}", config.version)?;
        writeln!(&mut release, "Component: main")?;
        writeln!(&mut release, "Origin: {}", config.origin)?;
        writeln!(&mut release, "Label: {}", config.label)?;
        writeln!(
            &mut release,
            "Architecture: {}",
            match arch.to_str().unwrap() {
                "binary-amd64" => "amd64",
                "binary-i386" => "i386",
                "binary-all" => "all",
                arch => panic!("unsupported architecture: {}", arch),
            }
        )?;
    }

    Ok(())
}

pub(crate) fn generate_sources_index(dist_base: &str, pool_base: &str) -> io::Result<()> {
    info!("generating sources index");
    let path = PathBuf::from([dist_base, "/main/source/"].concat());
    fs::create_dir_all(&path)?;

    let data = Command::new("apt-ftparchive")
        .arg("sources")
        .arg(PathBuf::from(pool_base).join("source"))
        .output()
        .map(|data| data.stdout)?;

    compress("Sources", &path, &data)
}

fn compress(name: &str, path: &Path, data: &[u8]) -> io::Result<()> {
    let mut uncompressed_data = File::create(path.join(name))?;
    uncompressed_data.write_all(data)?;

    let mut gz_file = File::create(path.join([name, ".gz"].concat()))?;
    let mut compressor = GzEncoder::new(&mut gz_file, Compression::Best);
    compressor.write_all(data)?;
    let _ = compressor.finish()?;

    let mut xz_file = File::create(path.join([name, ".xz"].concat()))?;
    let mut compressor = XzEncoder::new(data, 9);
    io::copy(&mut compressor, &mut xz_file).map(|_| ())
}

/// Generates the dists release file via `apt-ftparchive`.
pub(crate) fn generate_dists_release(config: &Config, base: &str) -> io::Result<()> {
    info!("generating dists release files");

    let cwd = env::current_dir()?;
    env::set_current_dir(base)?;

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
        .arg("APT::FTPArchive::Release::Architectures=i386 amd64 all")
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

    let mut release_file = File::create("Release")?;
    release_file.write_all(&release)?;
    env::set_current_dir(cwd)
}

/// Generates the `InRelease` file from the `Release` file via `gpg --clearsign`.
pub(crate) fn gpg_in_release(email: &str, release_path: &Path, out_path: &Path) -> io::Result<()> {
    info!("generating InRelease file");
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

/// Generates the `Release.gpg` file from the `Release` file via `gpg -abs`
pub(crate) fn gpg_release(email: &str, release_path: &Path, out_path: &Path) -> io::Result<()> {
    info!("generating Release.gpg file");
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
