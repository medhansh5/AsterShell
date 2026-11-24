//! AsterShell crate: configuration

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in configuration")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "configuration"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "configuration");
    }
}
