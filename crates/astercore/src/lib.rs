//! AsterShell crate: astercore

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in astercore")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "astercore"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "astercore");
    }
}
