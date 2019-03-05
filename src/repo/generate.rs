use checksum::hasher;
use config::Config;
use debian::{self, *};
use debarchive::Archive as DebArchive;
use md5::Md5;
use misc;
use rayon::{self, prelude::*};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use std::cmp::Ordering;
use std::collections::hash_map::{HashMap, Entry};
use std::{env, fs::{self, File}, io::{self, Write}, path::{Path, PathBuf}, process::{Command, Stdio}};
use deb_version::compare_versions;

use compress::*;

pub(crate) fn sources_index(component: &str, dist_base: &str, pool_base: &str) -> io::Result<()> {
    let pool_path = PathBuf::from(pool_base).join("source");
    if ! pool_path.exists() {
        return Ok(());
    }

    info!("generating sources index");
    let path = PathBuf::from([dist_base, "/", component, "/source/"].concat());
    fs::create_dir_all(&path)?;

    Command::new("apt-ftparchive")
        .arg("sources")
        .arg(pool_path)
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

// TODO: this can easily be replaced with Rust.
/// Generates the dists release file via `apt-ftparchive`.
pub(crate) fn dists_release(config: &Config, base: &str, components: &[String]) -> io::Result<()> {
    info!("generating dists release files");

    let cwd = env::current_dir()?;
    env::set_current_dir(base)?;

    let components = components.iter()
        .fold(String::new(), |mut acc, x| {
            acc.push_str(&x);
            acc.push(' ');
            acc
        });

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
        .arg(["APT::FTPArchive::Release::Components=", components.trim_right()].concat())
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

fn binary_suites(pool_base: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    Ok(fs::read_dir(pool_base)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let arch = entry.file_name();
            if &arch == "source" {
                None
            } else {
                let path = pool_base.join(&arch);
                let arch = match arch.to_str().unwrap() {
                    "binary-amd64" => "amd64",
                    "binary-i386" => "i386",
                    "binary-all" => "all",
                    arch => panic!("unsupported architecture: {}", arch),
                };

                Some((arch.to_owned(), path))
            }
        }).collect())
}

type ProcessedResults = Vec<io::Result<(PackageEntry, ContentsEntry, debian::Arch, debian::Component)>>;

pub(crate) fn dists(
    config: &Config,
    dist_base: &str,
    pool_base: &Path,
    components: &[String],
) -> io::Result<()> {
    info!("generating dist archives");

    let origin = &config.origin;

    // Collect the entries for each architecture of each component.
    let entries = components.par_iter().map(|component| {
        // Collect the entries for each architecture of this component
        binary_suites(&pool_base.join(&component)).unwrap()
            .into_par_iter()
            .map(|(arch, path)| {
                // Collect a list of packages to process for this architecture.
                // This list will have older entries filtered.
                let mut archives: HashMap<String, (String, PathBuf)> = HashMap::new();

                // An iterator that returns debian archives found in the path.
                let deb_iter = misc::walk_debs(&path, true)
                    .filter(|e| !e.file_type().is_dir())
                    .map(|e| e.path().to_path_buf());

                for package in deb_iter {
                    if let Some((name, version)) = get_debian_package_info(&package) {
                        match archives.entry(name) {
                            Entry::Occupied(mut entry) => {
                                if compare_versions(&entry.get().0, &version) == Ordering::Less {
                                    eprintln!("replacing {} with {}", entry.get().0, &version);
                                    entry.insert((version, package));
                                }
                            }
                            Entry::Vacant(mut entry) => {
                                entry.insert((version, package));
                            }
                        }
                    }
                }

                // Collect the entries for this architecture of this component
                archives.into_par_iter()
                    .map(|(_, (_, debian_entry))| {
                        info!("processing contents of {:?}", debian_entry);

                        let arch: &str = &arch;
                        let component: &str = &component;

                        // Open the Debian archive, and get the IDs & required codecs for the inner control and data archives.
                        let archive = DebArchive::new(&debian_entry)?;
                        // Open the control file within the control archive and read each key / value pair into a map.
                        let control = archive.control_map()?;

                        // The Contents archive requires that we know the package and section keys for each Debian package beforehand.
                        let package_name = match (control.get("Package"), control.get("Section")) {
                            (Some(ref package), Some(ref section)) if component == "main" => [section, "/", package].concat(),
                            (Some(ref package), Some(ref section)) => [component, "/", section, "/", package].concat(),
                            _ => {
                                return Err(io::Error::new(
                                    io::ErrorKind::Other,
                                    "did not find package + section from control archive"
                                ));
                            }
                        };

                        // Now get a listing of all the files for the Contents archive.
                        let mut files: Vec<PathBuf> = Vec::new();

                        // Runs each scope in parallel to generate the contents and checksums.
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
                                || archive.data(|entry| {
                                    let path = entry.path()?;
                                    files.push(path.to_path_buf());
                                    Ok(())
                                }),
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
                        let arch: String = arch.to_owned();
                        let component: String = component.to_owned();

                        Ok((package_entry, contents_entry, arch, component))
                    }).collect::<ProcessedResults>()
        }).collect::<Vec<ProcessedResults>>()
    }).collect::<Vec<Vec<ProcessedResults>>>();

    // Flatten the results for each architecture of each component into a single iterator.
    let entries = entries.into_iter()
        .flat_map(|entries| entries.into_iter().flat_map(|x| x.into_iter()));

    // Validate the results of each parallel process, and collect them in a manner so that they
    // may be used for further parallel processing and compression.
    let mut entries_map: debian::Entries = HashMap::new();
    for result in entries {
        let (package, contents, arch, component) = result?;

        match entries_map.entry(arch) {
            Entry::Occupied(mut entry) => {
                let entry = entry.get_mut();
                match (*entry).0.entry(component) {
                    Entry::Occupied(mut entry) => {
                        (*entry.get_mut()).push(package);
                    },
                    Entry::Vacant(mut entry) => {
                        entry.insert(vec![package]);
                    }
                }
                (*entry).1.push(contents);
            }
            Entry::Vacant(mut entry) => {
                entry.insert({
                    let mut component_map = HashMap::new();
                    component_map.insert(component, vec![package]);
                    (component_map, vec![contents])
                });
            }
        }
    }

    let destination = &Path::new(dist_base);
    let dist_files = DistFiles::new(destination, entries_map);
    // Re-enable duplicates checking.
    dist_files.compress_and_release(config, origin, None)
}
