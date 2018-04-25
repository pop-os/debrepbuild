extern crate failure;
extern crate rayon;
extern crate reqwest;
extern crate serde;
extern crate toml;

#[macro_use] extern crate failure_derive;
#[macro_use] extern crate serde_derive;

pub mod sources;
pub mod download;

use std::process::exit;

use download::DownloadResult;

fn main() {
    match sources::parse() {
        Ok(sources) => {
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
        }
        Err(why) => {
            eprintln!("repo-builder: {}", why);
            exit(1);
        }
    }
}
