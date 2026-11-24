//! AsterShell crate: search-core

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in search-core")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "search-core"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "search-core");
    }
}
