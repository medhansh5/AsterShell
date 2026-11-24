//! AsterShell crate: ui-components

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal error in ui-components")]
    Internal,
}

pub fn crate_name() -> &'static str {
    "ui-components"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "ui-components");
    }
}
