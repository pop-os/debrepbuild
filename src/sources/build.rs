use super::super::SHARED_ASSETS;
use super::artifacts::{link_artifact, LinkedArtifact};
use super::version::{changelog, git};
use super::SourceError;
use config::{DebianPath, Source};
use glob::glob;
use misc::{self, rsync};
use pool::mv_to_pool;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use subprocess::{Exec, Redirection};
use walkdir::WalkDir;

fn fetch_assets(
    linked: &mut Vec<LinkedArtifact>,
    src: &Path,
    dst: &Path,
) -> Result<(), SourceError> {
    for entry in WalkDir::new(src).into_iter().flat_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            let relative = path.strip_prefix(src).unwrap();
            let new_path = dst.join(relative);
            fs::create_dir(new_path);
        } else {
            linked.push(link_artifact(&path.canonicalize().unwrap(), dst)?);
        }
    }

    Ok(())
}

/// Attempts to build Debian packages from a given software repository.
pub fn build(item: &Source, pwd: &Path, branch: &str, force: bool) -> Result<(), SourceError> {
    info!("attempting to build {}", &item.name);
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
            if let Ok(globs) = glob(&[SHARED_ASSETS, &asset.src].concat()) {
                for file in globs.flat_map(|x| x.ok()) {
                    let dst = project_directory.join(&asset.dst);
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

    pre_flight(
        item,
        &pwd,
        branch,
        &project_directory,
        force,
    )?;

    let _ = env::set_current_dir("..");
    mv_to_pool("build", branch, item.keep_source).map_err(|why| SourceError::PackageMoving { why })
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
    item: &Source,
    pwd: &Path,
    branch: &str,
    dir: &Path,
    force: bool
) -> Result<(), SourceError> {
    let name = &item.name;
    let build_on = item.build_on.as_ref().map(|x| x.as_str());
    let record_path = PathBuf::from(["../record/", &name].concat());

    enum Record {
        Changelog(String),
        Commit(String, String),
        CommitAppend(String, String),
    }

    let record = match build_on {
        Some("changelog") => {
            let version = changelog(dir).map_err(|why| SourceError::Changelog { why })?;

            if !force && record_path.exists() {
                let record = misc::read_to_string(&record_path)
                    .map_err(|why| SourceError::RecordRead { why })?;
                let mut record = record.lines();

                if let Some(source) = record.next() {
                    if let Some(recorded_version) = record.next() {
                        if source == "changelog" && recorded_version == version {
                            info!("{} has already been built -- skipping", name);
                            return Ok(());
                        }
                    }
                }
            }

            info!("building {} at changelog version {}", name, version);
            Some(Record::Changelog(version))
        }
        Some("commit") => {
            let (branch, commit) = git(dir).map_err(|why| SourceError::GitVersion { why })?;
            let mut append = false;

            if !force && record_path.exists() {
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
                                    info!("{} has already been built -- skipping", name);
                                    return Ok(());
                                }
                            }
                        }
                        append = true;
                    }
                }
            }

            info!(
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

    sbuild(item, &pwd, branch, dir)?;

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

fn sbuild<P: AsRef<Path>>(
    item: &Source,
    pwd: &Path,
    branch: &str,
    path: P,
) -> Result<(), SourceError> {
    let stdout_file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(pwd.join(["logs/", &item.name, "-stdout"].concat()))
        .map_err(|why| SourceError::BuildCommand { why })?;

    let stderr_file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(pwd.join(["logs/", &item.name, "-stderr"].concat()))
        .map_err(|why| SourceError::BuildCommand { why })?;

    let mut command = Exec::cmd("sbuild")
        .stdout(Redirection::File(stdout_file))
        .stderr(Redirection::File(stderr_file));

    if let Some(ref depends) = item.depends {
        let mut temp = misc::walk_debs(&pwd.join(&["repo/pool/", branch, "/main"].concat()))
            .flat_map(|deb| misc::match_deb(&deb, depends))
            .collect::<Vec<(String, usize)>>();

        temp.sort_by(|a, b| a.1.cmp(&b.1));
        for &(ref p, _) in &temp {
            command = command.arg(&["--extra-package=", &p].concat());
        }
    }

    if let Some(commands) = item.prebuild.as_ref() {
        for cmd in commands {
            command = command.arg(&["--pre-build-commands=", &cmd].concat());
        }
    }

    if let Some(commands) = item.starting_build.as_ref() {
        for cmd in commands {
            command = command.arg(&["--starting-build-commands=", &cmd].concat());
        }
    }

    command = command.arg("-d");
    command = command.arg(branch);
    command = command.arg(path.as_ref());

    debug!("executing {:?}", command);

    let exit_status = command.join()
        .map_err(|why| SourceError::BuildCommand {
            why: io::Error::new(
                io::ErrorKind::Other,
                format!("{:?}", why)
            )
        })?;
    if exit_status.success() {
        info!("build succeeded!");
        Ok(())
    } else {
        error!("build failed!");
        Err(SourceError::BuildFailed)
    }
}
