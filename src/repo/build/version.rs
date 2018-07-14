use misc;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

type Branch = String;
type Commit = String;

pub fn git(project: &Path) -> io::Result<(Branch, Commit)> {
    let branch = misc::read_to_string(&project.join(".git/HEAD"))?;
    let branch = branch
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .split('/')
        .nth(2)
        .unwrap_or("");

    let commit = misc::read_to_string(&project.join(&[".git/refs/heads/", &branch].concat()))?;

    Ok((branch.to_owned(), commit))
}

pub fn changelog(member: &Path) -> io::Result<String> {
    File::open(member.join("debian/changelog"))
        .map(BufReader::new)
        .and_then(|mut buf| {
            let mut first_line = String::new();
            buf.read_line(&mut first_line)?;
            let version = first_line
                .split_whitespace()
                .nth(1)
                .map(|x| &x[1..x.len() - 1])
                .unwrap_or("");
            Ok(version.to_owned())
        })
}
