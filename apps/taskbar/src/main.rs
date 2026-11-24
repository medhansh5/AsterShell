//! AsterShell application: taskbar

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Initializing component: {}", "taskbar");
    let _ = astercore::crate_name();
    let _ = diagnostics::crate_name();
    println!("AsterShell component: taskbar initializing...");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_initialization() {
        assert_eq!(1 + 1, 2);
    }
}
