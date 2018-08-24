const FOUND: u8 = 1;

pub struct UrlTokenizer<'a> {
    data: &'a str,
    read: usize,
    flags: u8
}

#[derive(Debug, PartialEq)]
pub enum UrlToken<'a> {
    Name,
    Version,
    Unsupported(&'a str),
    Normal(&'a str)
}

impl<'a> UrlTokenizer<'a> {
    pub fn new(data: &'a str) -> UrlTokenizer<'a> {
        UrlTokenizer { data, read: 0, flags: 0 }
    }

    pub fn finalize(data: &'a str, name: &str, version: &str) -> Result<String, &'a str> {
        let mut output = String::with_capacity(data.len() * 2);
        for token in Self::new(data) {
            match token {
                UrlToken::Normal(text) => output.push_str(text),
                UrlToken::Name => output.push_str(name),
                UrlToken::Version => output.push_str(version),
                UrlToken::Unsupported(text) => return Err(text)
            }
        }

        output.shrink_to_fit();
        Ok(output)
    }
}

impl<'a> Iterator for UrlTokenizer<'a> {
    type Item = UrlToken<'a>;

    fn next(&mut self) -> Option<UrlToken<'a>> {
        if self.read >= self.data.len() {
            return None;
        }

        let mut start = self.read;
        let bytes = self.data.as_bytes();
        while self.read < self.data.len() - 1 {
            if self.flags == FOUND {
                if bytes[self.read] == b'}' {
                    let token = match &self.data[start..self.read] {
                        "name" => UrlToken::Name,
                        "version" => UrlToken::Version,
                        other => UrlToken::Unsupported(other)
                    };

                    self.read += 1;
                    self.flags = 0;
                    return Some(token);
                } else {
                    self.read += 1;
                }
            } else if &bytes[self.read..self.read + 2][..] == b"${" {
                let token = &self.data[start..self.read];
                self.flags = FOUND;
                self.read += 2;
                if !token.is_empty() {
                    return Some(UrlToken::Normal(token));
                } else {
                    start = self.read;
                }
            } else {
                self.read += 1;
            }
        }

        self.read = self.data.len();
        let remaining = &self.data[start..];
        if self.flags == FOUND {
            if remaining.len() > 1 {
                Some(match &remaining[..remaining.len() - 1] {
                    "name" => UrlToken::Name,
                    "version" => UrlToken::Version,
                    other => UrlToken::Unsupported(other)
                })
            } else {
                None
            }
        } else {
            Some(UrlToken::Normal(remaining))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_tokenizer() {
        let url = "https://app.domain.org/${name}/${name}_${version}.deb";
        assert_eq!(
            UrlTokenizer::new(url).collect::<Vec<_>>(),
            vec![
                UrlToken::Normal("https://app.domain.org/"),
                UrlToken::Name,
                UrlToken::Normal("/"),
                UrlToken::Name,
                UrlToken::Normal("_"),
                UrlToken::Version,
                UrlToken::Normal(".deb")
            ]
        );

        assert_eq!(
            UrlTokenizer::finalize(url, "system76", "1.0.0"),
            Ok("https://app.domain.org/system76/system76_1.0.0.deb".into())
        );

        assert_eq!(
            UrlTokenizer::finalize("https://app.domain.org/package_version.deb", "foo", "bar"),
            Ok("https://app.domain.org/package_version.deb".into())
        )
    }
}
