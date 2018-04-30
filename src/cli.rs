use std::env;

/// Possible actions that the user may request when running the application.
#[derive(Debug, PartialEq)]
pub enum Action {
    ConfigHelp,
    Fetch(String),
    FetchConfig,
    Unsupported,
    UpdatePackages,
    Update(String, String),
    UpdateRepository,
}

/// Checks the values that have been passed into the program, and returns the action
/// that the user requested.
pub fn requested_action() -> Action { get_action(env::args().skip(1)) }

/// Source code responsible for fetching an action from a given iterator.
/// Exists seperately from `requested_action` for testing purposes.
fn get_action<I: Iterator<Item = String>>(mut args: I) -> Action {
    match args.next().as_ref().map(|x| x.as_str()) {
        None => Action::UpdateRepository,
        Some("config") => match (
            args.next(),
            args.next().as_ref().map(|x| x.as_str()),
            args.next(),
        ) {
            (Some(key), Some("="), Some(value)) => Action::Update(key, value),
            (Some(key), None, None) => Action::Fetch(key),
            (None, None, None) => Action::FetchConfig,
            _ => Action::ConfigHelp,
        },
        Some("update") => Action::UpdatePackages,
        Some(_) => Action::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions() {
        assert_eq!(get_action(vec![].into_iter()), Action::UpdateRepository);

        assert_eq!(
            get_action(vec!["invalid".into()].into_iter()),
            Action::Unsupported
        );

        assert_eq!(
            get_action(vec!["config".into()].into_iter()),
            Action::FetchConfig
        );

        assert_eq!(
            get_action(vec!["config".into(), "archive".into()].into_iter()),
            Action::Fetch("archive".into())
        );

        assert_eq!(
            get_action(vec!["config".into(), "archive".into(), "=".into()].into_iter()),
            Action::ConfigHelp
        );

        assert_eq!(
            get_action(
                vec![
                    "config".into(),
                    "archive".into(),
                    "=".into(),
                    "value".into(),
                ].into_iter()
            ),
            Action::Update("archive".into(), "value".into())
        );
    }
}
