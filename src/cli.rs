use clap::ArgMatches;

/// Possible actions that the user may request when running the application.
#[derive(Debug, PartialEq)]
pub enum Action<'a> {
    Build(Vec<&'a str>, bool),
    Clean,
    Dist,
    Fetch(&'a str),
    FetchConfig,
    Migrate(Vec<&'a str>, &'a str, &'a str),
    Pool,
    Remove(Vec<&'a str>),
    Update(&'a str, &'a str),
    UpdateRepository,
}

impl<'a> Action<'a> {
    pub fn new(matches: &'a ArgMatches) -> Action<'a> {
        match matches.subcommand() {
            ("build", Some(build)) => match build.subcommand() {
                ("packages", Some(pkgs)) => Action::Build(
                    pkgs.values_of("packages").unwrap().collect(),
                    pkgs.is_present("force"),
                ),
                ("pool", _) => Action::Pool,
                ("dist", _) => Action::Dist,
                _ => Action::UpdateRepository,
            },
            ("clean", _) => Action::Clean,
            ("config", Some(config)) => config.value_of("key").map_or(Action::FetchConfig, |key| {
                config
                    .value_of("value")
                    .map_or(Action::Fetch(key), |value| Action::Update(key, value))
            }),
            ("remove", Some(pkgs)) => Action::Remove(pkgs.values_of("packages").unwrap().collect()),
            ("migrate", Some(migrate)) => Action::Migrate(
                migrate.values_of("packages").unwrap().collect(),
                migrate.value_of("from").unwrap(),
                migrate.value_of("to").unwrap(),
            ),
            _ => unreachable!(),
        }
    }
}
