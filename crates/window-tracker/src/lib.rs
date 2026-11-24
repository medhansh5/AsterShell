//! AsterShell crate: window-tracker

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in window-tracker")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "window-tracker"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "window-tracker");
    }
}
