use clap::ArgMatches;

/// Possible actions that the user may request when running the application.
#[derive(Debug, PartialEq)]
pub enum Action<'a> {
    ConfigHelp,
    Fetch(&'a str),
    FetchConfig,
    Unsupported,
    UpdatePackages,
    Update(&'a str, &'a str),
    UpdateRepository,
}

/// Checks the values that have been passed into the program, and returns the action
/// that the user requested.
pub fn requested_action<'a>(matches: &'a ArgMatches) -> Action<'a> {
    if let Some(build) = matches.subcommand_matches("build") {
        build.value_of("package")
            .map_or(Action::UpdateRepository, |_pkg| Action::Unsupported)
    } else if let Some(config) = matches.subcommand_matches("config") {
        config.value_of("key").map_or(Action::FetchConfig, |key| {
            config.value_of("value").map_or(Action::Fetch(key), |value| {
                Action::Update(key, value)
            })
        })
    } else {
        matches.subcommand_matches("update")
            .map_or(Action::ConfigHelp, |_| Action::UpdatePackages)
    }
}
