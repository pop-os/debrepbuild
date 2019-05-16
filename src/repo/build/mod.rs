mod artifacts;
mod extract;
mod metapackages;
mod rsync;

use command::Command;
use config::{Config, DebianPath, Direct, Source, SourceLocation};
use deb_version;
use debarchive::Archive as DebArchive;
use debian;
use glob::glob;
use misc;
use self::artifacts::{link_artifact, LinkedArtifact, LinkError};
use self::rsync::rsync;
use std::cmp::Ordering;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::exit;
use subprocess::{self, Exec, Redirection};
use super::pool::{mv_to_pool, KEEP_SOURCE};
use super::super::SHARED_ASSETS;
use super::version::{changelog, git};
use walkdir::WalkDir;

pub fn all(config: &Config) {
    let pwd = env::current_dir().unwrap();
    let suite = &config.archive;
    let component = &config.default_component;

    if let Some(ref sources) = config.source {
        migrate_to_pool(config, sources.iter());
        let build_path = ["build/", &config.archive].concat();
        for source in sources {
            if let Err(why) = build(config, source, &pwd, suite, component, false) {
                error!("package '{}' failed to build: {}", source.name, why);
                exit(1);
            }

            if let Err(why) = mv_to_pool(
                &build_path,
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
            let build_path = ["build/", &config.archive].concat();
            for source in &sources {
                if let Err(why) = build(config, source, &pwd, &config.archive, &config.default_component, force) {
                    error!("package '{}' failed to build: {}", source.name, why);
                    exit(1);
                }

                if let Err(why) = mv_to_pool(
                    &build_path,
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
    let build_path = ["build/", &config.archive].concat();
    for source in sources {
        if let Err(why) = mv_to_pool(
            &build_path,
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
    #[fail(display = "failed to set debian changelog: {}", why)]
    Debchange { why: io::Error },
    #[fail(display = "failed to create missing debian files for {:?}: {}", path, why)]
    DebFile { path: PathBuf, why: io::Error },
    #[fail(display = "failed to create directory for {:?}: {}", path, why)]
    Directory { path: PathBuf, why: io::Error },
    #[fail(display = "failed to move dsc files: {:?}", why)]
    DscMove { why: io::Error },
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
            if ! new_path.exists() {
                fs::create_dir_all(&new_path)
                    .map_err(|why| BuildError::Directory { path: new_path, why })?;
            }
        } else {
            let relative = path.strip_prefix(src).unwrap();

            let dst_: PathBuf;
            let dst = if relative.as_os_str().is_empty() {
                dst
            } else {
                dst_ = dst.join(relative);
                &dst_
            };

            let src = path.canonicalize().unwrap();
            linked.push(link_artifact(&src, &dst)?);
        }
    }

    Ok(())
}

/// Attempts to build Debian packages from a given software repository.
pub fn build(config: &Config, item: &Source, pwd: &Path, suite: &str, component: &str, force: bool) -> Result<(), BuildError> {
    info!("attempting to build {}", &item.name);
    let project_directory = pwd.join(&["build/", suite, "/", &item.name].concat());

    let mut dsc_file = None;

    match item.location {
        Some(SourceLocation::URL { ref url, .. }) => {
            if project_directory.exists() {
                let _ = fs::remove_dir_all(&project_directory);
            }

            let _ = fs::create_dir_all(&project_directory);
            let filename = misc::filename_from_url(url);
            let src = PathBuf::from(["assets/cache/", &item.name, "_", &filename].concat());
            let result = if item.extract {
                extract::extract(&src, &project_directory)
            } else {
                misc::copy(&src, &project_directory.join(filename))
            };

            result.map_err(|why| BuildError::Extract { src, dst: project_directory.clone(), why })?;
        }
        Some(SourceLocation::Dsc { ref dsc }) => {
            dsc_file = Some(misc::filename_from_url(dsc));
        }
        Some(SourceLocation::Git { ref commit, ref branch, .. }) => {
            debchange_git(suite, &config.version, &project_directory, branch, commit).map_err(|why| {
                BuildError::Debchange { why }
            })?;
        }
        _ => (),
    }

    // A list of hard-linked artifacts that will be removed at the end of the build.
    let mut linked: Vec<LinkedArtifact> = Vec::new();

    if dsc_file.is_none() {
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
                let debian_path = pwd.join(&["debian/", suite, "/", &item.name, "/"].concat());
                if debian_path.exists() {
                    let project_debian_path = project_directory.join("debian/");
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
                        // If the asset source is a directory, the filename of that directory
                        // will be appended to the destionation path.
                        let tmp: PathBuf;
                        let dst = if file.is_dir() {
                            tmp = asset.dst.join(file.file_name().unwrap());
                            &tmp
                        } else {
                            &asset.dst
                        };

                        // Then the destination will point to the build directory for this package.
                        let dst = project_directory.join(&dst);
                        if let Some(parent) = dst.parent() {
                            if ! parent.exists() {
                                fs::create_dir_all(&parent);
                            }
                        }

                        fetch_assets(&mut linked, &file, &dst)?;
                    }
                }
            }
        }
    }

    let _ = env::set_current_dir(&["build/", suite].concat());

    let skipped = pre_flight(
        config,
        item,
        &pwd,
        suite,
        component,
        dsc_file,
        &project_directory,
        force,
    )?;

    if !skipped && dsc_file.is_some() {
        misc::copy_here(&item.name).map_err(|why| {
            BuildError::DscMove { why }
        })?;
    }

    let _ = env::set_current_dir("../..");
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
    config: &Config,
    item: &Source,
    pwd: &Path,
    suite: &str,
    component: &str,
    dsc: Option<&str>,
    dir: &Path,
    force: bool
) -> Result<bool, BuildError> {
    let name = &item.name;
    let record_path = PathBuf::from(["../../record/", suite, "/", &name].concat());

    enum Record<'a> {
        Dsc(&'a str),
        Changelog(String),
        Commit(String, String),
        CommitAppend(String, String),
    }

    fn compare_record<F>(force: bool, record_path: &Path, mut compare: F) -> Result<bool, BuildError>
        where F: FnMut(::std::str::Lines) -> Result<bool, BuildError>
    {
        if !force && record_path.exists() {
            let record = misc::read_to_string(&record_path)
                .map_err(|why| BuildError::Read { file: record_path.to_owned(), why })?;
            return compare(record.lines())
        }

        Ok(false)
    }

    let mut skip = false;
    let record = if let Some(dsc) = dsc {
        skip = compare_record(force, &record_path, |mut lines| {
            if let (Some(source), Some(recorded_version)) = (lines.next(), lines.next()) {
                if source == "dsc" && recorded_version == dsc {
                    return Ok(true);
                }
            }

            info!("building {} at dsc version {}", name, dsc);
            Ok(false)
        })?;

        Some(Record::Dsc(dsc))
    } else {
        match item.build_on.as_ref().map(|x| x.as_str()) {
            Some("changelog") => {
                let version = changelog(&dir.join("debian/changelog"), 1)
                    .map_err(|why| BuildError::Changelog {
                        package: item.name.clone(),
                        why
                    }).and_then(|x| x.into_iter().next().ok_or_else(|| BuildError::NoChangelogVersion {
                        package: item.name.clone(),
                    }))?;

                skip = compare_record(force, &record_path, |mut lines| {
                    if let (Some(source), Some(recorded_version)) = (lines.next(), lines.next()) {
                        if source == "changelog" && recorded_version == version {
                            return Ok(true);
                        }
                    }

                    info!("building {} at changelog version {}", name, version);
                    Ok(false)
                })?;

                Some(Record::Changelog(version))
            }
            Some("commit") => {
                let (branch, commit) = git(dir).map_err(|why| BuildError::GitCommit {
                    package: item.name.clone(),
                    why
                })?;

                let mut append = &mut false;
                skip = compare_record(force, &record_path, |mut record| {
                    if let Some(source) = record.next() {
                        if source == "commit" {
                            for branch_entry in record {
                                let mut fields = branch_entry.split_whitespace();
                                if let (Some(rec_branch), Some(rec_commit)) =
                                    (fields.next(), fields.next())
                                {
                                    if rec_branch == branch && rec_commit == commit {
                                        return Ok(false);
                                    }
                                }
                            }
                            *append = true;
                        }
                    }

                    info!("building {} at git branch {}; commit {}", name, branch, commit);
                    Ok(false)
                })?;


                Some(if *append {
                    Record::CommitAppend(branch, commit)
                } else {
                    Record::Commit(branch, commit)
                })
            }
            Some(rule) => {
                return Err(BuildError::ConditionalRule { rule: rule.to_owned() });
            }
            None => None,
        }
    };

    if skip {
        info!("{} has already been built -- skipping", name);
        return Ok(true)
    }

    let path;
    let dir = match dsc {
        Some(dsc) => {
            path = dir.join(dsc);
            &path
        },
        None => dir
    };

    config
        .architectures
        .iter()
        .try_for_each(|arch| sbuild(config, item, &pwd, suite, component, dir, arch))?;

    let result = match record {
        Some(Record::Dsc(dsc)) => {
            misc::write(record_path, ["dsc\n", dsc].concat().as_bytes())
        }
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
        None => Ok(()),
    };

    result.map_err(|why| BuildError::RecordUpdate { package: item.name.to_string(), why })?;
    Ok(false)
}

fn sbuild<P: AsRef<Path>>(
    config: &Config,
    item: &Source,
    pwd: &Path,
    suite: &str,
    component: &str,
    path: P,
    arch: &str,
) -> Result<(), BuildError> {
    let log_path = pwd.join(["logs/", suite, "/", &item.name].concat());
    let mut command = Exec::cmd("sbuild")
        .args(&[
            "-v", "--log-external-command-output", "--log-external-command-error",
            &format!("--host={}", arch),
            // "--dpkg-source-opt=-Zgzip", // Use this when testing
            "-d", suite
        ])
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
        let pool = pwd.join(&["repo/pool/", suite, "/", component].concat());
        let deb_iter = misc::walk_debs(&pool, false)
            .flat_map(|deb| misc::match_deb(&deb, depends));

        let mut temp: Vec<(String, usize, String, String)> = Vec::new();
        for (deb, pos) in deb_iter {
            let (name, version) = debian::get_debian_package_info(&Path::new(&deb))
                .expect("failed to get debian name & version");

            let mut found = false;
            for stored_dep in &mut temp {
                if stored_dep.2 == name {
                    found = true;
                    if deb_version::compare_versions(&stored_dep.3, &version) == Ordering::Less {
                        stored_dep.0 = deb.clone();
                        stored_dep.1 = pos;
                        stored_dep.2 = name.clone();
                        stored_dep.3 = version.clone();
                        continue
                    }
                }
            }

            if ! found {
                temp.push((deb, pos, name, version));
            }
        }

        if depends.len() != temp.len() {
            for dependency in depends {
                if !temp.iter().any(|x| x.0.contains(dependency)) {
                    error!("dependency for {} not found: {}", path.as_ref().display(), dependency)
                }
            }

            return Err(BuildError::MissingDependencies);
        }

        temp.sort_by(|a, b| a.1.cmp(&b.1));
        for &(ref p, _, _, _) in &temp {
            command = command.arg(&["--extra-package=", &p].concat());
        }
    }

    for key in &config.extra_keys {
        command = command.arg(&format!("--extra-repository-key={}", key.display()));
    }

    if let Some(repos) = config.extra_repos.as_ref() {
        for repo in repos {
            command = command.arg(&["--extra-repository=", &repo].concat());
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

fn debchange_git(suite: &str, version: &str, project_directory: &Path, branch: &Option<String>, commit: &Option<String>) -> io::Result<()> {
    let commit_;
    let mut commit = match commit {
        Some(commit) => commit.trim(),
        None => {
            commit_ = Command::new("git")
                .arg("-C")
                .arg(project_directory)
                .arg("rev-parse")
                .arg(match branch {
                    Some(branch) => branch.as_str(),
                    None => "master"
                })
                .run_with_stdout()?;

            commit_.trim()
        }
    };

    let timestamp = Command::new("git")
        .arg("-C")
        .arg(project_directory)
        .args(&["show", "-s", "--format=%ct", commit])
        .run_with_stdout()?;

    if commit.len() > 6 {
        commit = &commit[..6];
    }

    Command::new("dch")
        .args(&[
            "-D", suite,
            "-l", &["~", timestamp.trim(), "~", version, "~", commit].concat(),
            "-c"
        ])
        .arg(&project_directory.join("debian/changelog"))
        .arg(&format!("automatic build of commit {}", commit))
        .run()
}
