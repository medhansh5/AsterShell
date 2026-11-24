//! AsterShell crate: search-providers

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in search-providers")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "search-providers"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "search-providers");
    }
}
