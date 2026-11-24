//! AsterShell crate: process-supervisor

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in process-supervisor")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "process-supervisor"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "process-supervisor");
    }
}
