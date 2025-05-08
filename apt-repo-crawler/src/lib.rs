extern crate chrono;
extern crate url_crawler;

pub use url_crawler::filename_from_url;

use chrono::{DateTime, FixedOffset};
use std::fmt;
use std::sync::Arc;
use url_crawler::*;

pub struct AptCrawler {
    crawler: Crawler
}

impl AptCrawler {
    pub fn new(repos: impl Into<CrawlerSource>) -> Self {
        AptCrawler {
            crawler: Crawler::new(repos)
        }
    }

    pub fn threads(self, threads: usize) -> Self {
        AptCrawler { crawler: self.crawler.threads(threads) } 
    }

    pub fn filter(self, filter: Arc<dyn AptPackageFilter>) -> Self {
        AptCrawler {
            crawler: self.crawler.pre_fetch(Arc::new(move |url| {
                if url.as_str().ends_with("/") { return true }
                AptPackage::from_str(url.as_str()).ok().map_or(false, |package| {
                    filter.validate(package)
                })
            }))
        }
    }

    pub fn crawl(self) -> AptCrawlIter {
        AptCrawlIter { iter: self.crawler.crawl() }
    }
}

pub struct AptCrawlIter {
    iter: CrawlIter
}

impl Iterator for AptCrawlIter {
    type Item = AptEntry;

    fn next(&mut self) -> Option<Self::Item> {
        for url in &mut self.iter {
            if let UrlEntry::File { url, content_type, length, modified } = url {
                return Some(AptEntry { url, content_type, length, modified });
            }
        }

        None
    }
}

#[derive(Debug)]
pub struct AptEntry {
    pub url: Url,
    pub content_type: String,
    pub length: u64,
    pub modified: Option<DateTime<FixedOffset>>
}

pub trait AptPackageFilter: Send + Sync {
    fn validate(&self, filter: AptPackage) -> bool;
}

#[derive(Debug)]
pub struct AptPackage<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub arch: &'a str,
    pub extension: &'a str
}

impl<'a> AptPackage<'a> {
    pub fn from_str(mut file_name: &'a str) -> Result<Self, ParseAptPackageError> {
        let mut pos = file_name.find('_').ok_or(ParseAptPackageError::NameNotFound)?;
        let name = &file_name[..pos];
        file_name = &file_name[pos+1..];

        pos = file_name.find('_').ok_or(ParseAptPackageError::VersionNotFound)?;
        let version = &file_name[..pos];
        file_name = &file_name[pos+1..];

        pos = file_name.find(".d")
            .or_else(|| file_name.find(".t"))
            .ok_or(ParseAptPackageError::InvalidExtension)?;

        let arch = &file_name[..pos];
        let extension = &file_name[pos+1..];

        Ok(AptPackage { name, version, arch, extension })
    }
}

#[derive(Debug)]
pub enum ParseAptPackageError {
    NameNotFound,
    VersionNotFound,
    InvalidExtension
}

impl fmt::Display for ParseAptPackageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "apt package's filename {}", match *self {
            ParseAptPackageError::NameNotFound => "does not have a name",
            ParseAptPackageError::VersionNotFound => "does not have a version",
            ParseAptPackageError::InvalidExtension => "does not have a proper extension",
        })
    }
}