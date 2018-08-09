use misc;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

type Branch = String;
type Commit = String;

pub fn git(project: &Path) -> io::Result<(Branch, Commit)> {
    let component = misc::read_to_string(&project.join(".git/HEAD"))?;
    let component = component
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .split('/')
        .nth(2)
        .unwrap_or("");

    let commit = misc::read_to_string(&project.join(&[".git/refs/heads/", &component].concat()))?;

    Ok((component.to_owned(), commit))
}

pub fn changelog(path: &Path, retain: usize) -> io::Result<Vec<String>> {
    File::open(path)
        .map(BufReader::new)
        .map(|buf| changelog_inner(buf.lines().filter_map(|x| x.ok()), retain))
}

fn changelog_inner<I: Iterator<Item = String>>(iter: I, retain: usize) -> Vec<String> {
    let iterator = iter.filter(|x| !x.starts_with(' '))
        .map(|x| {
            x.split_whitespace()
                .nth(1)
                .map(|x| &x[1..x.len() - 1])
                .unwrap_or("")
                .to_owned()
        }).filter(|x| !x.is_empty());

    if retain == 0 {
        iterator.collect()
    } else {
        iterator.take(retain).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog() {
        const TEST: &str = r#"system76-cuda-9.2 (0pop2) bionic; urgency=medium

  * Fix postinst rules

 -- Michael Murphy <michael@system76.com>  Mon, 16 Jul 2018 12:00:00 -0600

system76-cuda-9.2 (0pop1) bionic; urgency=medium

  * Initial release.

 -- Michael Murphy <michael@system76.com>  Mon, 25 Jun 2018 13:52:00 -0600"#;

        assert_eq!(
            changelog_inner(TEST.lines().map(|x| x.to_owned()), 0),
            vec!["0pop2".to_owned(), "0pop1".to_owned()]
        )
    }
}
