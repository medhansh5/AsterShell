//! AsterShell crate: platform-windows

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in platform-windows")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "platform-windows"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "platform-windows");
    }
}
