mod artifacts;
mod extract;
mod metapackages;
mod rsync;

use super::super::SHARED_ASSETS;
use self::artifacts::{link_artifact, LinkedArtifact, LinkError};
use super::version::{changelog, git};
use self::rsync::rsync;
use command::Command;
use config::{Config, DebianPath, Direct, Source, SourceLocation};
use debian;
use debarchive::Archive as DebArchive;
use glob::glob;
use misc;
use super::pool::{mv_to_pool, KEEP_SOURCE};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::exit;
use subprocess::{self, Exec, Redirection};
use walkdir::WalkDir;

pub fn all(config: &Config) {
    let pwd = env::current_dir().unwrap();
    let suite = &config.archive;
    let component = &config.default_component;

    if let Some(ref sources) = config.source {
        migrate_to_pool(config, sources.iter());
        for source in sources {
            if let Err(why) = build(source, &pwd, suite, component, false) {
                error!("package '{}' failed to build: {}", source.name, why);
                exit(1);
            }

            if let Err(why) = mv_to_pool(
                "build",
                &config.archive,
                &config.default_component,
                if source.keep_source { KEEP_SOURCE } else { 0 },
                Some(&source.name)
            ) {
                error!("package '{}' failed to migrate to pool: {}", source.name, why);
                exit(1);
            }
        }
    }

    if let Err(why) = repackage_binaries(config.direct.as_ref(), suite, component) {
        error!("binary repackage failure: {}", why);
        exit(1);
    }

    if let Err(why) = metapackages::generate(&config.archive, &config.default_component) {
        error!("metapackage generation failed: {}", why);
        exit(1);
    }
}

pub fn packages(config: &Config, packages: &[&str], force: bool) {
    let pwd = env::current_dir().unwrap();
    let mut built = 0;
    match config.source.as_ref() {
        Some(items) => {
            let sources = items.into_iter()
                .filter(|item| packages.contains(&item.name.as_str()))
                .collect::<Vec<&Source>>();

            migrate_to_pool(config, sources.iter().cloned());
            for source in &sources {
                if let Err(why) = build(source, &pwd, &config.archive, &config.default_component, force) {
                    error!("package '{}' failed to build: {}", source.name, why);
                    exit(1);
                }

                if let Err(why) = mv_to_pool(
                    "build",
                    &config.archive,
                    &config.default_component,
                    if source.keep_source { KEEP_SOURCE } else { 0 },
                    Some(&source.name)
                ) {
                    error!("package '{}' failed to migrate to pool: {}", source.name, why);
                    exit(1);
                }

                built += 1;
                if built == packages.len() {
                    break
                }
            }
        },
        None => warn!("no packages built")
    }
}

fn repackage_binaries(packages: Option<&Vec<Direct>>, suite: &str, component: &str) -> io::Result<()> {
    if let Some(packages) = packages {
        for package in packages {
            for destinations in package.get_destinations(suite, component).unwrap() {
                debug!("destinations: {:#?}", destinations);
                let pool = &destinations.pool;
                if let Some(&(ref files, ref source_deb)) = destinations.assets.as_ref() {
                    if needs_to_repackage(source_deb, files, pool)? {
                        repackage(source_deb, files, pool)?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// If source binary exists, and the files to replace are newer than the file in the pool, repackage.
fn needs_to_repackage(source: &Path, replace: &Path, pool: &Path) -> io::Result<bool> {
    info!("checking if {:?} needs to be repackaged", pool);
    if ! pool.exists() || ! source.exists() || ! replace.exists() {
        return Ok(true);
    }

    let timestamp_in_pool = pool.metadata()?.modified()?;
    for entry in WalkDir::new(replace).into_iter().flat_map(|e| e.ok()) {
        if entry.metadata()?.modified()? > timestamp_in_pool {
            return Ok(true);
        }
    }

    Ok(false)
}

fn repackage(source: &Path, replace: &Path, pool: &Path) -> io::Result<()> {
    info!("repackaging {:?}", pool);

    debug!("source: {:?}", source);
    debug!("replace: {:?}", replace);

    let data_replace = replace.join("data");
    let control_replace = replace.join("DEBIAN");

    if ! control_replace.exists() {
        fs::create_dir_all(&control_replace)?;
    }

    let parent = source.parent().unwrap();
    let data_dir = parent.join("data");
    let control_dir = parent.join("data/DEBIAN");

    if control_dir.exists() {
        fs::remove_dir_all(&control_dir)?;
    }

    if data_dir.exists() {
        fs::remove_dir_all(&data_dir)?;
    }

    fs::create_dir_all(&control_dir)?;

    let archive = DebArchive::new(source)?;
    archive.data_extract(&data_dir)?;
    archive.control_extract(&control_dir)?;

    if data_replace.exists() {
        rsync(&data_replace, &parent)?;
    }

    if control_replace.exists() {
        rsync(&control_replace, &data_dir)?;
    }
    
    fs::create_dir_all(pool.parent().unwrap())?;
    debian::archive::build(&data_dir, pool)?;

    Ok(())
}

fn migrate_to_pool<'a , I: Iterator<Item = &'a Source>>(config: &Config, sources: I) {
    for source in sources {
        if let Err(why) = mv_to_pool(
            "build",
            &config.archive,
            &config.default_component,
            if source.keep_source { KEEP_SOURCE } else { 0 },
            Some(&source.name)
        ) {
            error!("package '{}' failed to migrate to pool: {}", source.name, why);
            exit(1);
        }
    }
}

#[derive(Debug, Fail)]
pub enum BuildError {
    #[fail(display = "command for {} failed due to {:?}", package, reason)]
    Build { package: String, reason: subprocess::ExitStatus },
    #[fail(display = "failed to get changelog for {}: {}", package, why)]
    Changelog { package: String, why: io::Error },
    #[fail(display = "{} command failed to execute: {}", cmd, why)]
    Command { cmd: &'static str, why: io::Error },
    #[fail(display = "unsupported conditional build rule: {}", rule)]
    ConditionalRule { rule: String },
    #[fail(display = "failed to create missing debian files for {:?}: {}", path, why)]
    DebFile { path: PathBuf, why: io::Error },
    #[fail(display = "failed to create directory for {:?}: {}", path, why)]
    Directory { path: PathBuf, why: io::Error },
    #[fail(display = "failed to extract {:?} to {:?}: {}", src, dst, why)]
    Extract { src: PathBuf, dst: PathBuf, why: io::Error },
    #[fail(display = "failed to switch to branch {} on {}: {}", branch, package, why)]
    GitBranch { package: String, branch: String, why: io::Error },
    #[fail(display = "failed to get git commit for {}: {}", package, why)]
    GitCommit { package: String, why: io::Error },
    #[fail(display = "failed to link {:?} to {:?}: {}", src, dst, why)]
    Link { src: PathBuf, dst: PathBuf, why: io::Error },
    #[fail(display = "failed due to missing dependencies")]
    MissingDependencies,
    #[fail(display = "no version listed in changelog for {}", package)]
    NoChangelogVersion { package: String },
    #[fail(display = "failed to open file at {:?}: {}", file, why)]
    Open { file: PathBuf, why: io::Error },
    #[fail(display = "failed to read file at {:?}: {}", file, why)]
    Read { file: PathBuf, why: io::Error },
    #[fail(display = "failed to update record for {}: {}", package, why)]
    RecordUpdate { package: String, why: io::Error },
    #[fail(display = "rsyncing {:?} to {:?} failed: {}", src, dst, why)]
    Rsync { src: PathBuf, dst: PathBuf, why: io::Error },
}

impl From<LinkError> for BuildError {
    fn from(err: LinkError) -> BuildError {
        BuildError::Link { src: err.src, dst: err.dst, why: err.why }
    }
}

fn fetch_assets(
    linked: &mut Vec<LinkedArtifact>,
    src: &Path,
    dst: &Path,
) -> Result<(), BuildError> {
    for entry in WalkDir::new(src).into_iter().flat_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            let relative = path.strip_prefix(src).unwrap();
            let new_path = dst.join(relative);
            if !new_path.exists() {
                fs::create_dir(&new_path)
                    .map_err(|why| BuildError::Directory { path: new_path, why })?;
            }
        } else {
            let relative = path.strip_prefix(src).unwrap();
            let dst = dst.join(relative);
            let src = path.canonicalize().unwrap();
            linked.push(link_artifact(&src, &dst)?);
        }
    }

    Ok(())
}

/// Attempts to build Debian packages from a given software repository.
pub fn build(item: &Source, pwd: &Path, suite: &str, component: &str, force: bool) -> Result<(), BuildError> {
    info!("attempting to build {}", &item.name);
    let project_directory = pwd.join(&["build/", &item.name].concat());

    if let Some(SourceLocation::URL { ref url, .. }) = item.location {
        if project_directory.exists() {
            let _ = fs::remove_dir_all(&project_directory);
        }

        let _ = fs::create_dir_all(&project_directory);
        let filename = &url[url.rfind('/').map_or(0, |x| x + 1)..];
        let src = PathBuf::from(["assets/cache/", &item.name, "_", &filename].concat());
        extract::extract(&src, &project_directory)
            .map_err(|why| BuildError::Extract { src, dst: project_directory.clone(), why })?;
    }

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
                .map_err(|why| BuildError::GitBranch {
                    package: item.name.clone(),
                    branch: branch.clone(),
                    why
                })?;
        }
        None => {
            let debian_path = pwd.join(&["debian/", &item.name, "/"].concat());
            if debian_path.exists() {
                let project_debian_path = project_directory.join("debian");
                rsync(&debian_path, &project_debian_path)
                    .map_err(|why| BuildError::Rsync {
                        src: debian_path,
                        dst: project_debian_path.clone(),
                        why
                    })?;

                debian::create_missing_files(&project_debian_path)
                    .map_err(|why| BuildError::DebFile {
                        path: project_debian_path,
                        why
                    })?;
            }
        }
    }

    let _ = env::set_current_dir("build");

    pre_flight(
        item,
        &pwd,
        suite,
        component,
        &project_directory,
        force,
    )?;

    let _ = env::set_current_dir("..");
    Ok(())
}

fn merge_branch(url: &str, branch: &str) -> io::Result<()> {
    fs::create_dir_all("/tmp/debrep")?;
    fs::remove_dir_all("/tmp/debrep/repo")?;
    Command::new("git")
        .args(&["clone", "-b", branch, url, "/tmp/debrep/repo"])
        .run()?;

    Command::new("cp")
        .args(&["-r", "/tmp/debrep/repo/debian", "."])
        .run()
}

fn pre_flight(
    item: &Source,
    pwd: &Path,
    suite: &str,
    component: &str,
    dir: &Path,
    force: bool
) -> Result<(), BuildError> {
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
            let version = changelog(&dir.join("debian/changelog"), 1)
                .map_err(|why| BuildError::Changelog {
                    package: item.name.clone(),
                    why
                }).and_then(|x| x.into_iter().next().ok_or_else(|| BuildError::NoChangelogVersion {
                    package: item.name.clone(),
                }))?;

            if !force && record_path.exists() {
                let record = misc::read_to_string(&record_path)
                    .map_err(|why| BuildError::Read { file: record_path.clone(), why })?;
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
            let (branch, commit) = git(dir).map_err(|why| BuildError::GitCommit {
                package: item.name.clone(),
                why
            })?;

            let mut append = false;

            if !force && record_path.exists() {
                let record = misc::read_to_string(&record_path)
                    .map_err(|why| BuildError::Read { file: record_path.clone(), why })?;
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
            return Err(BuildError::ConditionalRule { rule: rule.to_owned() });
        }
        None => None,
    };

    sbuild(item, &pwd, suite, component, dir)?;

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

    result.map_err(|why| BuildError::RecordUpdate { package: item.name.to_string(), why })
}

fn sbuild<P: AsRef<Path>>(
    item: &Source,
    pwd: &Path,
    suite: &str,
    component: &str,
    path: P,
) -> Result<(), BuildError> {
    let log_path = pwd.join(["logs/", &item.name].concat());
    let mut command = Exec::cmd("sbuild")
        .args(&["-v", "--log-external-command-output", "--log-external-command-error", "-d", suite])
        .stdout(Redirection::Merge)
        .stderr(Redirection::File(
            fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(&log_path)
                .map_err(|why| BuildError::Open { file: log_path, why })?
        ));

    if let Some(ref depends) = item.depends {
        let mut temp = misc::walk_debs(&pwd.join(&["repo/pool/", suite, "/", component].concat()), false)
            .flat_map(|deb| misc::match_deb(&deb, depends))
            .collect::<Vec<(String, usize)>>();

        if depends.len() != temp.len() {
            for dependency in depends {
                if !temp.iter().any(|x| x.0.contains(dependency)) {
                    error!("dependency for {} not found: {}", path.as_ref().display(), dependency)
                }
            }

            return Err(BuildError::MissingDependencies);
        }

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

    command = command.arg(path.as_ref());

    debug!("executing {:#?}", command);

    let exit_status = command.join()
        .map_err(|why| BuildError::Command {
            cmd: "sbuild",
            why: io::Error::new(
                io::ErrorKind::Other,
                format!("{:?}", why)
            )
        })?;

    if exit_status.success() {
        Ok(())
    } else {
        Err(BuildError::Build {
            package: item.name.clone(),
            reason: exit_status
        })
    }
}
