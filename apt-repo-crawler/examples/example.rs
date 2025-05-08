extern crate apt_repo_crawler;

use apt_repo_crawler::*;
use std::sync::Arc;

pub struct Filter;

impl AptPackageFilter for Filter {
    fn validate(&self, package: AptPackage) -> bool {
        package.extension == "deb"
    }
}

pub fn main() {
    let crawler = AptCrawler::new(vec![
            "http://apt.pop-os.org/".to_owned(),
            "http://ppa.launchpad.net/mozillateam/ppa/ubuntu/pool/main/".to_owned()
        ]).filter(Arc::new(Filter));

    for file in crawler.crawl() {
        println!("{:#?}", file);
        println!("{:#?}", AptPackage::from_str(filename_from_url(file.url.as_str())));
    }
}
