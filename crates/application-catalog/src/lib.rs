//! AsterShell crate: application-catalog

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in application-catalog")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "application-catalog"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "application-catalog");
    }
}
