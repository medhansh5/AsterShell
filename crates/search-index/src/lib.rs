//! AsterShell crate: search-index

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in search-index")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "search-index"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "search-index");
    }
}
