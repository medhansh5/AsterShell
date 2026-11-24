//! AsterShell crate: shell-api

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in shell-api")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "shell-api"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "shell-api");
    }
}
