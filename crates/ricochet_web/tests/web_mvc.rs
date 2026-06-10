#[test]
fn active_record_database_url_smoke_allows_unset_or_postgres_url() {
    match std::env::var("DATABASE_URL") {
        Ok(url) => {
            assert!(
                url.starts_with("postgres://") || url.starts_with("postgresql://"),
                "DATABASE_URL must start with postgres:// or postgresql://"
            );
        }
        Err(std::env::VarError::NotPresent) => {
            println!("DATABASE_URL unset; skipping PostgreSQL Active Record smoke check");
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("DATABASE_URL must be valid Unicode");
        }
    }
}
