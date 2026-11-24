//! AsterShell crate: ipc

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in ipc")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "ipc"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "ipc");
    }
}
