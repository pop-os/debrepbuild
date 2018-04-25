extern crate deflate;
extern crate failure;
extern crate rayon;
extern crate reqwest;
extern crate serde;
extern crate toml;
extern crate xz2;

#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate serde_derive;

pub mod debian;
pub mod download;
pub mod sources;

use std::path::PathBuf;
use std::process::exit;

use download::DownloadResult;

fn main() {
    match sources::parse() {
        Ok(sources) => {
            eprintln!("DEBUG: Generated Config: {:#?}", sources);

            let ddl_sources = &sources.direct;
            let mut package_failed = false;
            for (id, result) in download::parallel(ddl_sources).into_iter().enumerate() {
                let name = &ddl_sources[id].name;
                match result {
                    Ok(DownloadResult::AlreadyExists) => {
                        eprintln!("package '{}' already exists", name);
                    }
                    Ok(DownloadResult::Downloaded(bytes)) => {
                        eprintln!("package '{}' successfully downloaded {} bytes", name, bytes);
                    }
                    Err(why) => {
                        eprintln!("package '{}' failed to download: {}", name, why);
                        package_failed = true;
                    }
                }
            }

            if package_failed {
                eprintln!("exiting due to error");
                exit(1);
            }

            if let Err(why) = debian::generate_binary_files(&sources, "amd64") {
                eprintln!("failed to generate files for binaries: {}", why);
                exit(1);
            }

            let release_path = PathBuf::from(["dists/", &sources.archive, "/Release"].concat());
            let in_release_path =
                PathBuf::from(["dists/", &sources.archive, "/InRelease"].concat());
            let release_gpg_path =
                PathBuf::from(["dists/", &sources.archive, "/Release.gpg"].concat());

            if let Err(why) = debian::generate_dists_release(&sources, &release_path) {
                eprintln!("failed to generate release file for dists: {}", why);
                exit(1);
            }

            if let Err(why) =
                debian::gpg_in_release(&sources.email, &release_path, &in_release_path)
            {
                eprintln!("failed to generate InRelease file: {}", why);
                exit(1);
            }

            if let Err(why) = debian::gpg_release(&sources.email, &release_path, &release_gpg_path)
            {
                eprintln!("failed to generate Release.gpg file: {}", why);
                exit(1);
            }
        }
        Err(why) => {
            eprintln!("debrepbuild: {}", why);
            exit(1);
        }
    }
}
