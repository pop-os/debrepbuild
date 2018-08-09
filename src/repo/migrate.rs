use config::Config;
use misc::{is_deb, INCLUDE_DDEB, INCLUDE_SRCS};
use std::io::{self, Error, ErrorKind};
use std::fs;
use std::path::PathBuf;
use std::os::unix::fs::MetadataExt;
use super::generate_release_files;
use walkdir::WalkDir;

pub fn migrate(config: &Config, packages: &[&str], from_component: &str, to_component: &str) -> io::Result<()> {
    info!("migrating {:?} from {} to {}", packages, from_component, to_component);

    let pool = ["repo/pool/", &config.archive, "/"].concat();
    let src_pool = PathBuf::from([&pool, from_component, "/"].concat());
    let dst_pool = PathBuf::from([&pool, to_component, "/"].concat());

    for package in packages {
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

        for file in files {
            let src_path = file.path();
            let dst_path = dst_pool.join(file.path().strip_prefix(&src_pool).unwrap());

            info!("migrating {} to {}", src_path.display(), dst_path.display());
            if let Some(dst_parent) = dst_path.parent() {
                if !dst_parent.exists() {
                    fs::create_dir_all(&dst_parent)?;
                }

                let src_metadata = fs::metadata(&src_path)?;
                let dst_metadata = fs::metadata(&dst_parent)?;
                if src_metadata.dev() == dst_metadata.dev() {
                    fs::rename(&src_path, &dst_path)?;
                } else {
                    fs::copy(&src_path, &dst_path)?;
                    fs::remove_file(&src_path)?;
                }
            }
        }
    }

    generate_release_files(&config)
        .map_err(|why| Error::new(ErrorKind::Other, format!("release file generation: {}", why)))
}
