use super::*;
use std::time::Duration;

fn test_url() -> String {
    let url = std::env::var("RICOCHET_POSTGRES_TEST_URL")
        .expect("set RICOCHET_POSTGRES_TEST_URL to a dedicated local test database");
    let config = url.parse::<PostgresConfig>().expect("valid test URL");
    assert!(postgres_hosts_are_local(&config));
    assert!(config
        .get_dbname()
        .is_some_and(|name| { name.starts_with("ricochet_test_") }));
    url
}

async fn pid(database: &PostgresDatabase) -> i32 {
    database
        .client
        .query_one("select pg_backend_pid()", &[])
        .await
        .expect("backend PID")
        .get(0)
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn committed_transactions_reuse_a_clean_physical_connection() {
    let database = PostgresDatabase::connect(&test_url()).await.unwrap();
    let first = database.begin_transaction().await.unwrap();
    let first_pid = pid(&first).await;
    first.commit_transaction().await.unwrap();
    drop(first);
    let second = database.begin_transaction().await.unwrap();
    assert_eq!(
        pid(&second).await,
        first_pid,
        "a completed connection must be reused"
    );
    second.rollback_transaction().await.unwrap();
}

async fn single_connection(recycle_timeout: Duration) -> PostgresDatabase {
    PostgresDatabase::connect_with_pool_options(
        &test_url(),
        PostgresPoolOptions {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(2),
            recycle_timeout,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn invalid_pool_limits_are_rejected_before_connecting() {
    for options in [
        PostgresPoolOptions {
            max_connections: 0,
            ..Default::default()
        },
        PostgresPoolOptions {
            max_connections: 65,
            ..Default::default()
        },
        PostgresPoolOptions {
            acquire_timeout: Duration::ZERO,
            ..Default::default()
        },
        PostgresPoolOptions {
            recycle_timeout: Duration::ZERO,
            ..Default::default()
        },
        PostgresPoolOptions {
            acquire_timeout: Duration::MAX,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            PostgresDatabase::connect_with_pool_options("invalid URL", options).await,
            Err(ActiveRecordError::Database {
                operation: "configure transaction pool",
                ..
            })
        ));
    }
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn session_state_and_session_locks_do_not_cross_transactions() {
    let database = single_connection(Duration::from_secs(1)).await;
    let first = database.begin_transaction().await.unwrap();
    let first_pid = pid(&first).await;
    first.execute_batch("set application_name = 'ricochet_pool_dirty'; set search_path = pg_catalog; create temp table ricochet_pool_temp(id int); prepare ricochet_pool_statement as select 1; listen ricochet_pool_channel; select pg_advisory_lock(907152021)").await.unwrap();
    first.commit_transaction().await.unwrap();
    drop(first);
    let next = database.begin_transaction().await.unwrap();
    assert_eq!(pid(&next).await, first_pid);
    let row = next.client.query_one("select current_setting('application_name'), to_regclass('pg_temp.ricochet_pool_temp') is null, (select count(*) from pg_prepared_statements where name = 'ricochet_pool_statement'), (select count(*) from pg_listening_channels())", &[]).await.unwrap();
    assert_ne!(row.get::<_, String>(0), "ricochet_pool_dirty");
    assert!(row.get::<_, bool>(1));
    assert_eq!(row.get::<_, i64>(2), 0);
    assert_eq!(row.get::<_, i64>(3), 0);
    let observer = database
        .client
        .query_one("select pg_try_advisory_lock(907152021)", &[])
        .await
        .unwrap();
    assert!(observer.get::<_, bool>(0), "session advisory lock leaked");
    database
        .client
        .simple_query("select pg_advisory_unlock(907152021)")
        .await
        .unwrap();
    next.rollback_transaction().await.unwrap();
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn dropping_an_unfinished_or_failed_transaction_rolls_back() {
    let database = single_connection(Duration::from_secs(1)).await;
    database
        .execute_batch("create table if not exists ricochet_pool_rollback_probe(marker bigint)")
        .await
        .unwrap();
    let marker = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    for failed in [false, true] {
        let transaction = database.begin_transaction().await.unwrap();
        let old_pid = pid(&transaction).await;
        transaction
            .execute_batch(&format!(
                "insert into ricochet_pool_rollback_probe values ({marker})"
            ))
            .await
            .unwrap();
        if failed {
            assert!(transaction.execute_batch("select 1/0").await.is_err());
        }
        drop(transaction);
        let next = database.begin_transaction().await.unwrap();
        assert_eq!(pid(&next).await, old_pid);
        let count: i64 = next
            .client
            .query_one(
                "select count(*) from ricochet_pool_rollback_probe where marker=$1",
                &[&marker],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 0, "unfinished transaction was committed or retained");
        next.rollback_transaction().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn stale_clones_cannot_query_and_keep_the_lease_exclusive() {
    let database = Arc::new(single_connection(Duration::from_secs(1)).await);
    let transaction = database.begin_transaction().await.unwrap();
    let old_pid = pid(&transaction).await;
    let retained = transaction.clone();
    transaction.commit_transaction().await.unwrap();
    assert!(retained.ping().await.is_err());
    assert!(retained.execute_batch("select 1").await.is_err());
    assert!(retained.commit_transaction().await.is_err());
    drop(transaction);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), database.begin_transaction())
            .await
            .is_err(),
        "retained handle's lease was returned early"
    );
    drop(retained);
    let next = database.begin_transaction().await.unwrap();
    assert_eq!(pid(&next).await, old_pid);
    next.rollback_transaction().await.unwrap();
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn capacity_is_bounded_and_cancelled_waiters_do_not_leak_permits() {
    let database = Arc::new(
        PostgresDatabase::connect_with_pool_options(
            &test_url(),
            PostgresPoolOptions {
                max_connections: 2,
                acquire_timeout: Duration::from_millis(200),
                ..Default::default()
            },
        )
        .await
        .unwrap(),
    );
    let first = database.begin_transaction().await.unwrap();
    let second = database.begin_transaction().await.unwrap();
    assert_ne!(pid(&first).await, pid(&second).await);
    let waiting_database = database.clone();
    let waiter = tokio::spawn(async move { waiting_database.begin_transaction().await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert!(matches!(
        database.begin_transaction().await,
        Err(ActiveRecordError::Database {
            operation: "acquire transaction connection",
            ..
        })
    ));
    drop(first);
    let replacement = database.begin_transaction().await.unwrap();
    replacement.rollback_transaction().await.unwrap();
    second.rollback_transaction().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn concurrent_requests_have_distinct_sessions_and_transactions() {
    let database = Arc::new(
        PostgresDatabase::connect_with_pool_options(
            &test_url(),
            PostgresPoolOptions {
                max_connections: 4,
                ..Default::default()
            },
        )
        .await
        .unwrap(),
    );
    let barrier = Arc::new(tokio::sync::Barrier::new(4));
    let mut tasks = Vec::new();
    for index in 0..4 {
        let database = database.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            let transaction = database.begin_transaction().await.unwrap();
            transaction
                .execute_batch(&format!(
                    "set application_name = 'ricochet_pool_request_{index}'"
                ))
                .await
                .unwrap();
            barrier.wait().await;
            for _ in 0..8 {
                let name: String = transaction
                    .client
                    .query_one("select current_setting('application_name')", &[])
                    .await
                    .unwrap()
                    .get(0);
                assert_eq!(name, format!("ricochet_pool_request_{index}"));
            }
            let row = transaction
                .client
                .query_one("select pg_backend_pid(), txid_current()::text", &[])
                .await
                .unwrap();
            let identity = (row.get::<_, i32>(0), row.get::<_, String>(1));
            transaction.commit_transaction().await.unwrap();
            identity
        }));
    }
    let mut sessions = std::collections::BTreeSet::new();
    let mut transactions = std::collections::BTreeSet::new();
    for task in tasks {
        let (session, transaction) = task.await.unwrap();
        sessions.insert(session);
        transactions.insert(transaction);
    }
    assert_eq!(sessions.len(), 4);
    assert_eq!(transactions.len(), 4);
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn cancelled_query_is_drained_before_reuse() {
    let database = single_connection(Duration::from_secs(1)).await;
    let transaction = database.begin_transaction().await.unwrap();
    let old_pid = pid(&transaction).await;
    assert!(tokio::time::timeout(
        Duration::from_millis(20),
        transaction.execute_batch("select pg_sleep(0.15)")
    )
    .await
    .is_err());
    drop(transaction);
    let next = database.begin_transaction().await.unwrap();
    assert_eq!(pid(&next).await, old_pid);
    next.ping().await.unwrap();
    next.rollback_transaction().await.unwrap();
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn stalled_reset_evicts_the_driver_and_releases_capacity() {
    let database = single_connection(Duration::from_millis(40)).await;
    let transaction = database.begin_transaction().await.unwrap();
    let old_pid = pid(&transaction).await;
    assert!(tokio::time::timeout(
        Duration::from_millis(20),
        transaction.execute_batch("select pg_sleep(3)")
    )
    .await
    .is_err());
    drop(transaction);
    let next = database.begin_transaction().await.unwrap();
    assert_ne!(pid(&next).await, old_pid);
    next.ping().await.unwrap();
    next.rollback_transaction().await.unwrap();
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn terminated_connections_are_discarded_and_replaced() {
    let database = single_connection(Duration::from_secs(1)).await;
    let transaction = database.begin_transaction().await.unwrap();
    let old_pid = pid(&transaction).await;
    let killed: bool = database
        .client
        .query_one("select pg_terminate_backend($1)", &[&old_pid])
        .await
        .unwrap()
        .get(0);
    assert!(killed);
    assert!(transaction.ping().await.is_err());
    drop(transaction);
    let next = database.begin_transaction().await.unwrap();
    assert_ne!(pid(&next).await, old_pid);
    next.rollback_transaction().await.unwrap();
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn failed_commit_is_terminal_and_the_connection_recovers() {
    let database = single_connection(Duration::from_secs(1)).await;
    let transaction = database.begin_transaction().await.unwrap();
    let old_pid = pid(&transaction).await;
    transaction.execute_batch("create temporary table ricochet_pool_deferred(id int, unique(id) deferrable initially deferred); insert into ricochet_pool_deferred values(1),(1)").await.unwrap();
    assert!(transaction.commit_transaction().await.is_err());
    assert!(transaction.ping().await.is_err());
    drop(transaction);
    let next = database.begin_transaction().await.unwrap();
    assert_eq!(pid(&next).await, old_pid);
    next.ping().await.unwrap();
    next.rollback_transaction().await.unwrap();
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn cancelled_finish_waiting_for_a_cloned_query_preserves_the_transaction() {
    let database = single_connection(Duration::from_secs(1)).await;
    for commit in [true, false] {
        let transaction = database.begin_transaction().await.unwrap();
        let clone = transaction.clone();
        let query = tokio::spawn(async move {
            clone.execute_batch("select pg_sleep(0.15)").await.unwrap();
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while transaction
                .transaction_active("test")
                .unwrap()
                .try_write()
                .is_ok()
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let finish = async {
            if commit {
                transaction.commit_transaction().await
            } else {
                transaction.rollback_transaction().await
            }
        };
        assert!(tokio::time::timeout(Duration::from_millis(20), finish)
            .await
            .is_err());
        query.await.unwrap();
        transaction
            .ping()
            .await
            .expect("cancellation before finish executes is recoverable");
        let clone = transaction.clone();
        let finished = tokio::spawn(async move {
            if commit {
                clone.commit_transaction().await
            } else {
                clone.rollback_transaction().await
            }
        });
        finished.await.unwrap().unwrap();
        assert!(
            transaction.ping().await.is_err(),
            "stale clone must reject queries after finish"
        );
    }
}

#[tokio::test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
async fn cancelled_in_flight_finishes_are_terminal_and_recycle_safely() {
    let database = single_connection(Duration::from_secs(1)).await;
    for commit in [true, false] {
        let transaction = database.begin_transaction().await.unwrap();
        let old_pid = pid(&transaction).await;
        if commit {
            transaction.execute_batch("create temporary table ricochet_pool_slow_commit(id int); create function pg_temp.ricochet_pool_slow_finish() returns trigger language plpgsql as $$ begin perform pg_sleep(0.15); return null; end $$; create constraint trigger ricochet_pool_deferred after insert on ricochet_pool_slow_commit deferrable initially deferred for each row execute function pg_temp.ricochet_pool_slow_finish(); insert into ricochet_pool_slow_commit values(1)").await.unwrap();
        } else {
            assert!(tokio::time::timeout(
                Duration::from_millis(20),
                transaction.execute_batch("select pg_sleep(0.15)")
            )
            .await
            .is_err());
        }
        let finish = async {
            if commit {
                transaction.commit_transaction().await
            } else {
                transaction.rollback_transaction().await
            }
        };
        assert!(tokio::time::timeout(Duration::from_millis(20), finish)
            .await
            .is_err());
        assert!(transaction.ping().await.is_err());
        drop(transaction);
        let next = database.begin_transaction().await.unwrap();
        assert_eq!(
            pid(&next).await,
            old_pid,
            "cleanup must drain the cancelled finish before reuse"
        );
        next.ping().await.unwrap();
        next.rollback_transaction().await.unwrap();
    }
}

#[tokio::test]
async fn cancelled_connection_startup_releases_its_slot() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (accepted, mut connections) = tokio::sync::mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let mut sockets = Vec::new();
        for index in 1..=2 {
            sockets.push(listener.accept().await.unwrap().0);
            accepted.send(index).unwrap();
        }
        std::future::pending::<()>().await;
        drop(sockets);
    });
    let config =
        format!("host=127.0.0.1 port={port} dbname=ricochet_test_fake user=fake sslmode=disable")
            .parse::<PostgresConfig>()
            .unwrap();
    let pool = PostgresPool::new(
        Arc::new(config),
        PostgresPoolOptions {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(1),
            recycle_timeout: Duration::from_millis(100),
        },
    );
    for expected in 1..=2 {
        // The fake peer accepts TCP but deliberately never answers startup.
        assert!(
            tokio::time::timeout(Duration::from_millis(40), pool.acquire())
                .await
                .is_err()
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), connections.recv())
                .await
                .unwrap(),
            Some(expected)
        );
    }
    server.abort();
}

#[tokio::test]
async fn failed_begin_and_failed_reset_evict_the_connection_without_losing_capacity() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (evicted, mut evictions) = tokio::sync::mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let evicted = evicted.clone();
            tokio::spawn(async move {
                // A minimal trusted peer injects server errors that cannot reliably
                // be induced against a real database without racing backend death.
                let size = socket.read_u32().await.unwrap();
                assert!((8..10_000).contains(&size));
                let mut startup = vec![0; size as usize - 4];
                socket.read_exact(&mut startup).await.unwrap();
                socket
                    .write_all(b"R\0\0\0\x08\0\0\0\0Z\0\0\0\x05I")
                    .await
                    .unwrap();
                let mut reset_failed = false;
                loop {
                    let Ok(tag) = socket.read_u8().await else {
                        if reset_failed {
                            evicted.send(()).unwrap();
                        }
                        break;
                    };
                    if tag == b'X' {
                        break;
                    }
                    assert_eq!(tag, b'Q');
                    let size = socket.read_u32().await.unwrap();
                    assert!((5..10_000).contains(&size));
                    let mut sql = vec![0; size as usize - 4];
                    socket.read_exact(&mut sql).await.unwrap();
                    let response = if sql == b"rollback\0" {
                        b"C\0\0\0\x0dROLLBACK\0".to_vec()
                    } else {
                        assert!(sql == b"begin\0" || sql == b"discard all\0");
                        reset_failed |= sql == b"discard all\0";
                        let fields = b"SERROR\0CXX000\0Mdeliberate test fault\0\0";
                        let mut response = vec![b'E'];
                        response.extend_from_slice(&((fields.len() + 4) as u32).to_be_bytes());
                        response.extend_from_slice(fields);
                        response
                    };
                    socket.write_all(&response).await.unwrap();
                    socket.write_all(b"Z\0\0\0\x05I").await.unwrap();
                }
            });
        }
    });
    let database = PostgresDatabase::connect_with_pool_options(
        &format!("host=127.0.0.1 port={port} dbname=ricochet_test_fake user=fake sslmode=disable"),
        PostgresPoolOptions {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(1),
            recycle_timeout: Duration::from_secs(1),
        },
    )
    .await
    .unwrap();
    for _ in 0..2 {
        assert!(database.begin_transaction().await.is_err());
        tokio::time::timeout(Duration::from_secs(1), evictions.recv())
            .await
            .unwrap()
            .unwrap();
    }
    drop(database);
    server.abort();
}

#[test]
#[ignore = "requires a dedicated local PostgreSQL RICOCHET_POSTGRES_TEST_URL"]
fn dropping_after_runtime_shutdown_does_not_poison_pool_capacity() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let database = runtime.block_on(single_connection(Duration::from_secs(1)));
    let transaction = runtime.block_on(database.begin_transaction()).unwrap();
    drop(runtime);
    drop(transaction);
    let replacement_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    replacement_runtime.block_on(async {
        let next = database.begin_transaction().await.unwrap();
        next.ping().await.unwrap();
        next.rollback_transaction().await.unwrap();
    });
}
