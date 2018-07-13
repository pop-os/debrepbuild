use clap::ArgMatches;

/// Possible actions that the user may request when running the application.
#[derive(Debug, PartialEq)]
pub enum Action<'a> {
    Build(&'a str, bool),
    Fetch(&'a str),
    FetchConfig,
    Update(&'a str, &'a str),
    UpdateRepository,
}

impl<'a> Action<'a> {
    fn new(matches: &'a ArgMatches) -> Action<'a> {
        match matches.subcommand() {
            ("build", Some(build)) => match build.value_of("package") {
                Some(pkg) => Action::Build(pkg, build.is_present("force")),
                None => Action::UpdateRepository
            }
            ("config", Some(config)) => {
                config.value_of("key").map_or(Action::FetchConfig, |key| {
                    config.value_of("value").map_or(Action::Fetch(key), |value| {
                        Action::Update(key, value)
                    })
                })
            }
            _ => unreachable!()
        }
    }
}
