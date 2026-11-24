//! AsterShell application: start-menu

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Initializing component: {}", "start-menu");
    let _ = astercore::crate_name();
    let _ = diagnostics::crate_name();
    println!("AsterShell component: start-menu initializing...");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_initialization() {
        assert_eq!(1 + 1, 2);
    }
}
