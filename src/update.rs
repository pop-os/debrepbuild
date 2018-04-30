use config::{Config, ParsingError};
use reqwest::{self, Client, Url};
use select::document::Document;
use select::predicate::Name;

impl From<ParsingError> for UpdateError {
    fn from(why: ParsingError) -> UpdateError {
        UpdateError::Write { why }
    }
}

#[derive(Debug, Fail)]
pub enum UpdateError {
    #[fail(display = "unable to request page: {}", why)]
    Request { why: reqwest::Error },
    #[fail(display = "invalid URL: {}", why)]
    InvalidURL { why: reqwest::UrlError },
    #[fail(display = "version string not found in link '{}'", link)]
    NoVersion { link: String },
    #[fail(display = "unable to write config to disk: {}", why)]
    Write { why: ParsingError },
    #[fail(display = "package not found for '{}'", package)]
    NotFound { package: String }
}

pub fn update_packages(sources: &mut Config) -> Result<(), UpdateError> {
    let client = Client::new();
    'outer: for direct in &mut sources.direct {
        if let Some(ref update) = direct.update {
            match update.source.as_str() {
                "directory" => {
                    let response = client.get(&update.url)
                        .send()
                        .map_err(|why| UpdateError::Request { why })?;

                    let document = Document::from_read(response).unwrap();

                    let urls = document
                        .find(Name("a"))
                        .filter_map(|n| n.attr("href"))
                        .filter_map(|n| match update.contains {
                            Some(ref contains) => if n.contains(contains) {
                                Some(n)
                            } else {
                                None
                            },
                            None => Some(n)
                        })
                        .collect::<Vec<&str>>();

                    for link in urls.into_iter().rev() {
                        if link.ends_with(match direct.arch.as_str() {
                            "amd64" => "amd64.deb",
                            "i386" => "i386.deb",
                            _ => ".deb"
                        }) {
                            match between(&link, &update.after, &update.before) {
                                Some(version) => {
                                    let url = if update.url.ends_with('/') {
                                        [&update.url, link].concat()
                                    } else {
                                        [&update.url, "/", link].concat()
                                    };

                                    direct.version = version.to_owned();
                                    direct.url = url.to_string();

                                    eprintln!("updated {}:\n  version: {}\n  url: {}", direct.name, version, url);
                                    continue 'outer
                                }
                                None => {
                                    return Err(UpdateError::NoVersion { link: link.to_owned() });
                                }
                            }
                        }
                    }
                },
                "github" => {
                    let url = ["https://github.com/", &update.url, "/releases/latest/"].concat();
                    let response = client.get(&url)
                        .send()
                        .map_err(|why| UpdateError::Request { why })?;

                    let document = Document::from_read(response).unwrap();

                    let urls = document
                        .find(Name("a"))
                        .filter_map(|n| n.attr("href"))
                        .filter_map(|n| match update.contains {
                            Some(ref contains) => if n.contains(contains) {
                                Some(n)
                            } else {
                                None
                            },
                            None => Some(n)
                        });

                    for link in urls {
                        if link.ends_with(".deb") {
                            match between(&link, &update.after, &update.before) {
                                Some(version) => {
                                    let url = if link.starts_with("https:/") || link.starts_with("http:/") {
                                        link.to_owned()
                                    } else {
                                        let mut url = Url::parse(&url)
                                            .map_err(|why| UpdateError::InvalidURL { why })?;

                                        url.set_path(&link);
                                        url.to_string()
                                    };

                                    direct.version = version.to_owned();
                                    direct.url = url.to_string();

                                    eprintln!("updated {}:\n  version: {}\n  url: {}", direct.name, version, url);
                                    continue 'outer
                                }
                                None => {
                                    return Err(UpdateError::NoVersion { link: link.to_owned() });
                                }
                            }
                        }
                    }

                    return Err(UpdateError::NotFound { package: direct.name.clone() });
                },
                _ => ()
            }
        } else {
            eprintln!("warning: {} requires manual updating", direct.name);
        }
    }

    sources.write_to_disk()?;
    Ok(())
}

fn get_after<'a>(origin: &'a str, after: &str) -> Option<&'a str> {
    origin.find(after)
        .map(|pos| origin.split_at(pos + after.len()))
        .map(|(_, origin)| origin)
}

fn get_before<'a>(origin: &'a str, before: &str) -> Option<&'a str> {
    origin.find(before)
        .map(|pos| origin.split_at(pos))
        .map(|(origin, _)| origin)
}

fn between<'a>(origin: &'a str, after: &str, before: &str) -> Option<&'a str> {
    get_after(origin, after).and_then(|origin| get_before(origin, before))
}
