use std::path::Path;

pub fn get_debian_package_info(package: &Path) -> Option<(String, String)> {
    let mut filename = package.file_name()?.to_str()?;
    let mut underscore_pos = filename.find('_')?;
    let name = &filename[..underscore_pos];
    filename = &filename[underscore_pos + 1..];
    underscore_pos = filename.find('_')?;
    Some((
        if filename.ends_with("ddeb") {
            [name, "_d"].concat()
        } else {
            name.to_owned()
        },
        filename[..underscore_pos].to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn debian_version() {
        let package = Path::new("/some_/pa_th/to/name_version_arch.deb");
        assert_eq!(
            get_debian_package_info(&package),
            Some(("name".to_owned(), "version".to_owned()))
        );

        let package = Path::new("/some_/pa_th/to/name_version_arch.ddeb");
        assert_eq!(
            get_debian_package_info(&package),
            Some(("name_d".to_owned(), "version".to_owned()))
        );
    }
}
