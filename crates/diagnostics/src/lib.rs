//! AsterShell crate: diagnostics

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in diagnostics")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "diagnostics"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "diagnostics");
    }
}
