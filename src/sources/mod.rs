use std::io;

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
    #[fail(display = "git command failed")]
    GitFailed,
    #[fail(display = "unable to git '{}': {}", item, why)]
    GitRequest { item: String, why:  io::Error },
    #[fail(display = "unsupported cvs for source: {}", cvs)]
    UnsupportedCVS { cvs: String },
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
}
