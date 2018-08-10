use config::Config;
use misc::{is_deb, INCLUDE_DDEB, INCLUDE_SRCS};
use std::io::{self, Error};
use std::fs;
use std::path::{Path, PathBuf};
use std::os::unix::fs::MetadataExt;
use super::{generate_release_files, ReleaseError};
use walkdir::WalkDir;

#[derive(Debug, Fail)]
pub enum MigrationError {
    #[fail(display = "failed to migrate {:?} to {:?}: {}", src, dst, why)]
    Move { src: PathBuf, dst: PathBuf, why: Error },
    #[fail(display = "dist release file generation failed: {}", why)]
    DistRelease { why: ReleaseError }
}

impl From<ReleaseError> for MigrationError {
    fn from(why: ReleaseError) -> Self {
        MigrationError::DistRelease { why }
    }
}

pub fn migrate(config: &Config, packages: &[&str], from_component: &str, to_component: &str) -> Result<(), MigrationError> {
    info!("migrating {:?} from {} to {}", packages, from_component, to_component);
    inner_migrate(config, packages, from_component, to_component)?;
    generate_release_files(&config)?;
    Ok(())
}

fn inner_migrate(config: &Config, packages: &[&str], from_component: &str, to_component: &str) -> Result<(), MigrationError> {
    let pool = ["repo/pool/", &config.archive, "/"].concat();
    let src_pool = PathBuf::from([&pool, from_component, "/"].concat());
    let dst_pool = PathBuf::from([&pool, to_component, "/"].concat());

    packages.into_iter().map(|package| {
        let files = WalkDir::new(&src_pool)
            .min_depth(1)
            .max_depth(4)
            .into_iter()
            .filter_entry(|e| match e.depth() {
                1 | 2 => true,
                3 => &e.file_name() == package,
                4 => is_deb(e, INCLUDE_DDEB | INCLUDE_SRCS),
                _ => false
            })
            .flat_map(|e| e.ok())
            .filter(|e| e.depth() == 4);

        files.into_iter().map(|file| {
            let src = file.path();
            let dst = dst_pool.join(file.path().strip_prefix(&src_pool).unwrap());

            migrate_file(&src, &dst).map_err(|why| MigrationError::Move {
                src: src.to_path_buf(),
                dst,
                why
            })
        }).collect::<Result<(), MigrationError>>()
    }).collect()
}

fn migrate_file(src_path: &Path, dst_path: &Path) -> io::Result<()> {
    if let Some(dst_parent) = dst_path.parent() {
        info!("migrating {} to {}", src_path.display(), dst_path.display());
        if !dst_parent.exists() {
            fs::create_dir_all(&dst_parent)?;
        }

        let src_metadata = fs::metadata(src_path)?;
        let dst_metadata = fs::metadata(&dst_parent)?;
        if src_metadata.dev() == dst_metadata.dev() {
            fs::rename(src_path, dst_path)?;
        } else {
            fs::copy(src_path, dst_path)?;
            fs::remove_file(src_path)?;
        }
    }

    Ok(())
}
