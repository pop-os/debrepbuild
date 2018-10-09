use apt_repo_crawler::{AptPackage, AptPackageFilter};
use regex::Regex;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Repo {
    pub repo: String,
    pub version: Option<RepoPattern>,
    pub arch: Option<RepoPattern>,
    pub name: Option<RepoPattern>
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct RepoPattern {
    pub not: Option<String>,
    pub is: Option<String>,
}

impl AptPackageFilter for Repo {
    fn validate(&self, package: AptPackage) -> bool {
        if ! match_pattern(&self.version, package.version) {
            return false;
        }

        if ! match_pattern(&self.arch, package.arch) {
            return false;
        }

        if ! match_pattern(&self.name, package.name) {
            return false;
        }

        true
    }
}

fn match_pattern(filter: &Option<RepoPattern>, input: &str) -> bool {
    if let Some(version) = filter {
        if let Some(ref version) = version.is {
            if ! match_regex(version, input) {
                return false
            }
        }
        
        if let Some(ref version) = version.not {
            if match_regex(version, input) {
                return false
            }
        }
    }

    true
}

fn match_regex(regex: &str, input: &str) -> bool {
    match Regex::new(regex) {
        Ok(regex) => regex.is_match(input),
        Err(why) => {
            eprintln!("invalid regex: '{}': {}", regex, why);
            false
        }
    }
}