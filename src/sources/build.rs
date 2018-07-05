use super::super::misc;
use super::artifacts::{link_artifact, LinkedArtifact};
use super::version::{changelog, git};
use super::SourceError;
use config::{Source, SourceMember};
use glob::glob;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Attempts to build Debian packages from a given software repository.
pub fn build(item: &Source, path: &Path, branch: &str) -> Result<(), SourceError> {
    eprintln!("building {}", path.display());
    let pwd = env::current_dir().unwrap();
    let cwd = pwd.join(path);

    let mut linked: Vec<LinkedArtifact> = Vec::new();

    if let Some(ref artifacts) = item.artifacts {
        for artifact in artifacts {
            if let Ok(globs) = glob(&["assets/", &artifact.src].concat()) {
                for file in globs.flat_map(|x| x.ok()) {
                    let src = file.canonicalize().unwrap();
                    let dst = path.join(&artifact.dst);
                    linked.push(link_artifact(&src, &dst)?);
                }
            }
        }
    }

    let _ = fs::create_dir_all("build/record");
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

    if let Some(ref members) = item.members {
        let mut sorted_members: Vec<SourceMember> = members.clone();
        sorted_members.sort_by(|a, b| a.priority.cmp(&b.priority));

        for member in sorted_members {
            let cwd = cwd.join(&member.directory);
            pre_flight(
                branch,
                &member.name,
                &cwd,
                member.build_on.as_ref().map(|x| x.as_str()),
            )?;
        }
    } else {
        pre_flight(
            branch,
            &item.name,
            &cwd,
            item.build_on.as_ref().map(|x| x.as_str()),
        )?;
    };

    let _ = env::set_current_dir("..");
    misc::mv_to_pool("build").map_err(|why| SourceError::PackageMoving { why })
}

fn pre_flight(
    branch: &str,
    name: &str,
    dir: &Path,
    build_on: Option<&str>,
) -> Result<(), SourceError> {
    let record_path = PathBuf::from(["record/", &name].concat());

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

    sbuild(branch, dir)?;

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

fn sbuild<P: AsRef<Path>>(branch: &str, path: P) -> Result<(), SourceError> {
    let exit_status = Command::new("sbuild")
        .arg("-j4")
        .arg("-d")
        .arg(branch)
        .arg(path.as_ref())
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
