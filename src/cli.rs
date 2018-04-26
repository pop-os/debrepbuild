use std::env;

pub enum Action {
    UpdateRepository,
    Fetch(String),
    Update(String, String),
    ConfigHelp,
    Unsupported,
}

pub fn requested_action() -> Action {
    let mut args = env::args().skip(1);
    match args.next().as_ref().map(|arg| arg.as_str() == "config") {
        None => Action::UpdateRepository,
        Some(true) => match (
            args.next(),
            args.next().as_ref().map(|x| x.as_str()),
            args.next(),
        ) {
            (Some(key), Some("="), Some(value)) => Action::Update(key, value),
            (Some(key), None, None) => Action::Fetch(key),
            _ => Action::ConfigHelp,
        },
        Some(false) => Action::Unsupported,
    }
}
