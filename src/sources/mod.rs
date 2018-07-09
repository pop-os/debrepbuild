use std::io;
use std::path::PathBuf;
use reqwest;

mod artifacts;
pub mod build;
pub mod download;
mod version;

#[derive(Debug, Fail)]
pub enum SourceError {
    #[fail(display = "build command failed: {}", why)]
    BuildCommand { why: io::Error },
    #[fail(display = "failed to build from source")]
    BuildFailed,
    #[fail(display = "failed to switch to branch {}: {}", branch, why)]
    GitBranch { branch: String, why: io::Error },
    #[fail(display = "git command failed")]
    GitFailed,
    #[fail(display = "unable to git '{}': {}", item, why)]
    GitRequest { item: String, why:  io::Error },
    #[fail(display = "unable to create link to artifact: {}", why)]
    Link { why: io::Error },
    #[fail(display = "artifact link could not be removed: {}", why)]
    LinkRemoval { why: io::Error },
    #[fail(display = "failed to move package artifact to pool: {}", why)]
    PackageMoving { why: io::Error },
    #[fail(display = "unable to get changelog details: {}", why)]
    Changelog { why: io::Error },
    #[fail(display = "unable to read contents of record: {}", why)]
    RecordRead { why: io::Error },
    #[fail(display = "unable to update record: {}", why)]
    RecordUpdate { why: io::Error },
    #[fail(display = "unsupported conditional build rule: {}", rule)]
    UnsupportedConditionalBuild { rule: String },
    #[fail(display = "unable to get git branch / commit: {}", why)]
    GitVersion { why: io::Error },
    #[fail(display = "I/O error with '{:?}': {}", file, why)]
    File { file: PathBuf, why: io::Error },
    #[fail(display = "unable to download '{}': {}", item, why)]
    Request { item: String, why: reqwest::Error },
    #[fail(display = "downloaded archive has an invalid checksum: expected {}, received {}", expected, received)]
    InvalidChecksum { expected: String, received: String },
    #[fail(display = "failed to extract tar at '{:?}': {}", path, why)]
    TarExtract { path: PathBuf, why: io::Error },
    #[fail(display = "rsync failed: {}", why)]
    Rsync { why: io::Error }
}
