//! AsterShell Search Index Subsystem
//!
//! Embedded SQLite FTS5 database foundation for launch history, frecency weighting,
//! metadata caching, and privacy-preserving index clearing.

pub mod database;
pub mod error;
pub mod frecency;
pub mod schema;

pub use database::{SearchDatabase, SearchHistoryItem};
pub use error::{IndexError, Result};
pub use frecency::calculate_frecency;

pub fn crate_name() -> &'static str {
    "search-index"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_database_initialization_and_migration() {
        let db = SearchDatabase::open_in_memory().expect("Should initialize in-memory SQLite");
        assert!(db.get_recent_history(10).unwrap().is_empty());
    }

    #[test]
    fn test_record_usage_and_frecency_calculation() {
        let db = SearchDatabase::open_in_memory().expect("Should open database");
        let cat = search_core::SearchResultCategory::Application;

        // Record first launch
        db.record_usage("term", "exe:c:\\windows\\wt.exe", "Windows Terminal", &cat)
            .expect("Record usage should succeed");

        let score1 = db
            .get_frecency_score("exe:c:\\windows\\wt.exe")
            .expect("Query frecency")
            .expect("Should have frecency");

        assert!(score1 > 0.0);

        // Record second launch
        db.record_usage("term", "exe:c:\\windows\\wt.exe", "Windows Terminal", &cat)
            .expect("Second usage should succeed");

        let history = db.get_recent_history(10).expect("Get recent history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].use_count, 2);
        assert_eq!(history[0].title, "Windows Terminal");
        assert!(history[0].frecency_score >= score1);
    }

    #[test]
    fn test_fts5_history_matching() {
        let db = SearchDatabase::open_in_memory().expect("Should open database");
        let cat = search_core::SearchResultCategory::Application;
        db.record_usage("code", "exe:c:\\vs\\code.exe", "Visual Studio Code", &cat)
            .unwrap();
        db.record_usage("calc", "exe:calc.exe", "Windows Calculator", &cat)
            .unwrap();

        let matches = db
            .search_history_fts("studio", 5)
            .expect("FTS search should succeed");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].title, "Visual Studio Code");
    }

    #[test]
    fn test_clear_history_and_privacy_deletion() {
        let db = SearchDatabase::open_in_memory().expect("Should open database");
        let cat = search_core::SearchResultCategory::Application;
        db.record_usage("priv", "exe:app1.exe", "Private App", &cat)
            .unwrap();
        db.record_usage("work", "exe:app2.exe", "Work App", &cat)
            .unwrap();

        assert_eq!(db.get_recent_history(10).unwrap().len(), 2);

        // Delete single target
        let deleted = db.delete_target("exe:app1.exe").expect("Delete target");
        assert!(deleted);
        assert_eq!(db.get_recent_history(10).unwrap().len(), 1);

        // Clear all history
        let cleared_count = db.clear_history().expect("Clear history");
        assert_eq!(cleared_count, 1);
        assert!(db.get_recent_history(10).unwrap().is_empty());
    }

    #[test]
    fn test_corrupt_database_recovery() {
        let temp_dir =
            std::env::temp_dir().join(format!("astershell_test_db_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let db_file = temp_dir.join("test_search.db");
        let cat = search_core::SearchResultCategory::Application;

        // Write corrupt garbage into the database file
        std::fs::write(
            &db_file,
            b"THIS IS NOT A VALID SQLITE DATABASE FILE GARBAGE CONTENT",
        )
        .unwrap();

        // Opening should detect corruption, quarantine it, and reinitialize a clean database
        let db = SearchDatabase::open(&db_file).expect("Should recover corrupt database");
        assert!(db.get_recent_history(10).unwrap().is_empty());

        // Verify write works on recovered DB
        db.record_usage("test", "exe:test.exe", "Test App", &cat)
            .unwrap();
        assert_eq!(db.get_recent_history(10).unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
