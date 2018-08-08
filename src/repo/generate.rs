use config::Config;
use misc;
use rayon::prelude::*;
use rayon;
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use debian::DebianArchive;
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use md5::Md5;
use checksum::hasher;
use debian::*;

use compress::*;

pub(crate) fn sources_index(branch: &str, dist_base: &str, pool_base: &str) -> io::Result<()> {
    info!("generating sources index");
    let path = PathBuf::from([dist_base, "/", branch, "/source/"].concat());
    fs::create_dir_all(&path)?;

    Command::new("apt-ftparchive")
        .arg("sources")
        .arg(PathBuf::from(pool_base).join("source"))
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            {
                let stdout = child.stdout.as_mut().unwrap();
                compress("Sources", &path, stdout, UNCOMPRESSED | GZ_COMPRESS | XZ_COMPRESS)?;
            }

            child.wait().and_then(|stat| {
                if stat.success() {
                    Ok(())
                } else {
                    Err(io::Error::new(io::ErrorKind::Other, "apt-ftparchive failed"))
                }
            })
        })
}

/// Generates the dists release file via `apt-ftparchive`.
pub(crate) fn dists_release(config: &Config, base: &str) -> io::Result<()> {
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
        .arg(["APT::FTPArchive::Release::Components=", &config.default_branch].concat())
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

pub(crate) fn contents(config: &Config, dist_base: &str, suites: &[(String, PathBuf)]) -> io::Result<()> {
    info!("generating content archives");

    let branch = &config.default_branch;
    let origin = &config.origin;

    suites.par_iter().map(|&(ref arch, ref path)| {
        // Collects a list of deb packages to read, and then reads them in parallel.
        let entries: Vec<io::Result<(PackageEntry, ContentsEntry)>> = misc::walk_debs(&path, true)
            .filter(|e| !e.file_type().is_dir())
            .map(|e| e.path().to_path_buf())
            .collect::<Vec<PathBuf>>()
            .into_par_iter()
            .map(|debian_entry| {
                info!("processing contents of {:?}", debian_entry);

                // Open the Debian archive, and get the IDs & required codecs for the inner control and data archives.
                let archive = DebianArchive::new(&debian_entry)?;
                // Open the control file within the control archive and read each key / value pair into a map.
                let control = archive.control()?;

                // The Contents archive requires that we know the package and section keys for each Debian package beforehand.
                let package_name = match (control.get("Package"), control.get("Section")) {
                    (Some(ref package), Some(ref section)) if branch == "main" => [section, "/", package].concat(),
                    (Some(ref package), Some(ref section)) => [branch, "/", section, "/", package].concat(),
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "did not find package + section from control archive"
                        ));
                    }
                };

                // Now get a listing of all the files for the Contents archive.
                let mut files = Vec::new();

                let (content_res, ((sha1_res, sha256_res), (sha512_res, md5_res))) = {
                    let path = &debian_entry;
                    // TODO: use bus_writer instead of reading the same file in each thread.
                    let generate_hashes = || {
                        rayon::join(
                            || rayon::join(
                                || File::open(path).and_then(hasher::<Sha1, File>),
                                || File::open(path).and_then(hasher::<Sha256, File>),
                            ),
                            || rayon::join(
                                || File::open(path).and_then(hasher::<Sha512, File>),
                                || File::open(path).and_then(hasher::<Md5, File>),
                            )
                        )
                    };

                    rayon::join(
                        || archive.data(|path| files.push(path.to_path_buf())),
                        generate_hashes
                    )
                };

                drop(archive);
                content_res?;
                let package_entry = PackageEntry {
                    control,
                    filename: debian_entry.clone(),
                    size: File::open(&debian_entry).and_then(|file| file.metadata().map(|m| m.len()))?,
                    md5sum: md5_res?,
                    sha1: sha1_res?,
                    sha256: sha256_res?,
                    sha512: sha512_res?,
                };

                let contents_entry = ContentsEntry { package: package_name, files };

                Ok((package_entry, contents_entry))
            }).collect();


        let destination = &Path::new(dist_base);
        let dist_files = DistFiles::new(destination, &arch, entries, origin, None)?;
        dist_files.check_for_duplicates();
        dist_files.compress_and_release(config)
    }).collect()
}
