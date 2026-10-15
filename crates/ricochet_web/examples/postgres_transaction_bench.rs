//! Local, read-only transaction benchmark. Requires a dedicated test database.
use std::time::{Duration, Instant};

use ricochet_web::PostgresDatabase;
use serde_json::json;
use tokio_postgres::{config::Host, Client, Config, NoTls};

async fn sessions(client: &Client) -> i64 {
    client
        .simple_query("select pg_stat_clear_snapshot()")
        .await
        .unwrap();
    client
        .query_one(
            "select sessions from pg_stat_database where datname=current_database()",
            &[],
        )
        .await
        .unwrap()
        .get(0)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let url = std::env::var("RICOCHET_POSTGRES_TEST_URL").expect("dedicated test URL required");
    let config = url.parse::<Config>().unwrap();
    assert!(config
        .get_dbname()
        .is_some_and(|name| name.starts_with("ricochet_test_")));
    assert!(!config.get_hosts().is_empty());
    assert!(config.get_hosts().iter().all(|host| {
        matches!(host, Host::Tcp(name) if name == "127.0.0.1" || name == "localhost" || name == "::1")
    }));
    let (monitor, driver) = config.connect(NoTls).await.unwrap();
    tokio::spawn(async move {
        driver.await.unwrap();
    });
    let before = sessions(&monitor).await;
    let database = PostgresDatabase::connect(&url).await.unwrap();
    let mut timings = Vec::new();
    for index in 0..61 {
        let start = Instant::now();
        let transaction = database.begin_transaction().await.unwrap();
        transaction.ping().await.unwrap();
        transaction.rollback_transaction().await.unwrap();
        drop(transaction);
        timings.push(json!({"index": index, "warmup": index == 0, "wall_ms": start.elapsed().as_secs_f64()*1000.0}));
        // The next checkout includes the preceding connection's asynchronous reset.
    }
    drop(database);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let after = sessions(&monitor).await;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "transactions": 61, "measured_after_warmup": 60,
            "sessions_before": before, "sessions_after": after,
            "sessions_delta_including_root_connection": after-before, "timings": timings
        }))
        .unwrap()
    );
}
