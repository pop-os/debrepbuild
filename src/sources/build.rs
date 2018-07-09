use super::super::SHARED_ASSETS;
use super::artifacts::{link_artifact, LinkedArtifact};
use super::version::{changelog, git};
use super::SourceError;
use config::{DebianPath, Source};
use glob::glob;
use misc::{self, rsync};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

fn fetch_assets(
    linked: &mut Vec<LinkedArtifact>,
    src: &Path,
    dst: &Path,
) -> Result<(), SourceError> {
    let directory = src.read_dir()
        .map_err(|why| SourceError::File { file: src.to_path_buf(), why })?;

    for entry in directory {
        if let Ok(entry) = entry {
            linked.push(link_artifact(&entry.path().canonicalize().unwrap(), dst)?);
        }
    }

    Ok(())
}

/// Attempts to build Debian packages from a given software repository.
pub fn build(item: &Source, pwd: &Path, branch: &str) -> Result<(), SourceError> {
    eprintln!("attempting to build {}", &item.name);
    let project_directory = pwd.join(&["build/", &item.name].concat());
    let _ = fs::create_dir_all(&project_directory);

    let mut linked: Vec<LinkedArtifact> = Vec::new();

    match pwd.join(&["assets/packages/", &item.name].concat()) {
        ref local_assets if local_assets.exists() => {
            fetch_assets(&mut linked, local_assets, &project_directory)?;
        },
        _ => ()
    }

    if let Some(ref assets) = item.assets {
        for asset in assets {
            eprintln!("asset: {:#?}", asset);
            if let Ok(globs) = glob(&[SHARED_ASSETS, &asset.src].concat()) {
                for file in globs.flat_map(|x| x.ok()) {
                    let dst = project_directory.join(&asset.dst);
                    eprintln!("{:?} -> {:?}", file, dst);
                    linked.push(link_artifact(&file, &dst)?);
                }
            }
        }
    }

    match item.debian {
        Some(DebianPath::URL { ref url, ref checksum }) => {
            unimplemented!()
        }
        Some(DebianPath::Branch { ref url, ref branch }) => {
            merge_branch(url, branch)
                .map_err(|why| SourceError::GitBranch { branch: branch.clone(), why })?;
        }
        None => {
            match pwd.join(&["debian/", &item.name, "/"].concat()) {
                ref debian_path if debian_path.exists() => {
                    rsync(debian_path, &project_directory.join("debian"))
                        .map_err(|why| SourceError::Rsync { why })?;
                }
                _ => ()
            }
        }
    }

    let _ = env::set_current_dir("build");

    if let Some(ref prebuild) = item.prebuild {
        eprintln!("prebuilding {}", item.name);
        for command in prebuild {
            let exit_status = Command::new("sh")
                .args(&["-c", command])
                .status()
                .map_err(|why| SourceError::BuildCommand { why })?;

            if !exit_status.success() {
                return Err(SourceError::BuildFailed);
            }
        }
    }

    let packages: Vec<String> = match item.depends {
        Some(ref depends) => {
            let mut temp = misc::walk_debs(&pwd.join(&["repo/pool/", branch, "/main"].concat()))
                .flat_map(|deb| misc::match_deb(&deb, depends))
                .collect::<Vec<(String, usize)>>();

            temp.sort_by(|a, b| a.1.cmp(&b.1));
            temp.into_iter().map(|x| x.0).collect::<Vec<String>>()
        }
        None => Vec::new()
    };

    pre_flight(
        branch,
        &item.name,
        &project_directory,
        item.build_on.as_ref().map(|x| x.as_str()),
        &packages,
    )?;

    let _ = env::set_current_dir("..");
    misc::mv_to_pool("build", branch).map_err(|why| SourceError::PackageMoving { why })
}

fn merge_branch(url: &str, branch: &str) -> io::Result<()> {
    fs::create_dir_all("/tmp/debrep")?;
    fs::remove_dir_all("/tmp/debrep/repo")?;
    Command::new("git")
        .args(&["clone", "-b", branch, url, "/tmp/debrep/repo"])
        .status()?;

    Command::new("cp")
        .args(&["-r", "/tmp/debrep/repo/debian", "."])
        .status()?;

    Ok(())
}

fn pre_flight(
    branch: &str,
    name: &str,
    dir: &Path,
    build_on: Option<&str>,
    packages: &[String]
) -> Result<(), SourceError> {
    let record_path = PathBuf::from(["../record/", &name].concat());

    enum Record {
        Changelog(String),
        Commit(String, String),
        CommitAppend(String, String),
    }

    let record = match build_on {
        Some("changelog") => {
            let version = changelog(dir).map_err(|why| SourceError::Changelog { why })?;

            if record_path.exists() {
                let record = misc::read_to_string(&record_path)
                    .map_err(|why| SourceError::RecordRead { why })?;
                let mut record = record.lines();

                if let Some(source) = record.next() {
                    if let Some(recorded_version) = record.next() {
                        if source == "changelog" && recorded_version == version {
                            println!("{} has already been built -- skipping", name);
                            return Ok(());
                        }
                    }
                }
            }

            println!("building {} at changelog version {}", name, version);
            Some(Record::Changelog(version))
        }
        Some("commit") => {
            let (branch, commit) = git(dir).map_err(|why| SourceError::GitVersion { why })?;
            let mut append = false;

            if record_path.exists() {
                let record = misc::read_to_string(&record_path)
                    .map_err(|why| SourceError::RecordRead { why })?;
                let mut record = record.lines();

                if let Some(source) = record.next() {
                    if source == "commit" {
                        for branch_entry in record {
                            let mut fields = branch_entry.split_whitespace();
                            if let (Some(rec_branch), Some(rec_commit)) =
                                (fields.next(), fields.next())
                            {
                                if rec_branch == branch && rec_commit == commit {
                                    println!("{} has already been built -- skipping", name);
                                    return Ok(());
                                }
                            }
                        }
                        append = true;
                    }
                }
            }

            println!(
                "building {} at git branch {}; commit {}",
                name, branch, commit
            );
            Some(if append {
                Record::CommitAppend(branch, commit)
            } else {
                Record::Commit(branch, commit)
            })
        }
        Some(rule) => {
            return Err(SourceError::UnsupportedConditionalBuild {
                rule: rule.to_owned(),
            });
        }
        None => None,
    };

    sbuild(branch, dir, packages)?;

    let result = match record {
        Some(Record::Changelog(version)) => {
            misc::write(record_path, ["changelog\n", &version].concat().as_bytes())
        }
        Some(Record::Commit(branch, commit)) => misc::write(
            record_path,
            ["commit\n", &branch, " ", &commit].concat().as_bytes(),
        ),
        Some(Record::CommitAppend(branch, commit)) => OpenOptions::new()
            .create(true)
            .append(true)
            .open(record_path)
            .and_then(|mut file| file.write_all([&branch, " ", &commit].concat().as_bytes())),
        None => return Ok(()),
    };

    result.map_err(|why| SourceError::RecordUpdate { why })
}

fn sbuild<P: AsRef<Path>>(branch: &str, path: P, extra_packages: &[String]) -> Result<(), SourceError> {
    let mut command = Command::new("sbuild");
    for p in extra_packages {
        command.arg(&["--extra-package=", p].concat());
    }
    command.arg("-d");
    command.arg(branch);
    command.arg(path.as_ref());

    eprintln!("DEBUG: {:?}", command);

    let exit_status = command
        .status()
        .map_err(|why| SourceError::BuildCommand { why })?;

    if exit_status.success() {
        eprintln!("build succeeded!");
        Ok(())
    } else {
        eprintln!("build failed!");
        Err(SourceError::BuildFailed)
    }
}
