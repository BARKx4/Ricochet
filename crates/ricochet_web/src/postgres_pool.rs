use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use tokio::task::AbortHandle;
use tokio_postgres::{Client, Config};

use crate::active_record::{connect_postgres_client_with_driver, ActiveRecordError};

/// Bounds for transaction connections. The shared autocommit connection is separate.
#[derive(Clone, Copy, Debug)]
pub struct PostgresPoolOptions {
    pub max_connections: usize,
    /// Total time allowed for waiting, recycling, and opening a connection.
    pub acquire_timeout: Duration,
    /// Time allowed for rollback and complete session reset on return.
    pub recycle_timeout: Duration,
}

impl Default for PostgresPoolOptions {
    fn default() -> Self {
        Self {
            max_connections: 8,
            acquire_timeout: Duration::from_secs(10),
            recycle_timeout: Duration::from_secs(5),
        }
    }
}

impl PostgresPoolOptions {
    pub(crate) fn validate(self) -> Result<(), ActiveRecordError> {
        if !(1..=64).contains(&self.max_connections)
            || self.acquire_timeout.is_zero()
            || self.recycle_timeout.is_zero()
            || self.acquire_timeout > Duration::from_secs(300)
            || self.recycle_timeout > Duration::from_secs(300)
        {
            return Err(pool_error(
                "configure transaction pool",
                "capacity must be 1..=64 and timeouts must be positive and at most 300 seconds",
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct PostgresPool(Arc<PoolInner>);

struct PoolInner {
    config: Arc<Config>,
    options: PostgresPoolOptions,
    idle: Mutex<Vec<Arc<PoolConnection>>>,
    permits: Arc<Semaphore>,
    recycling: AtomicUsize,
    returned: Notify,
}

struct PoolConnection {
    client: Arc<Client>,
    driver: AbortHandle,
}

impl Drop for PoolConnection {
    fn drop(&mut self) {
        // Eviction closes the socket even if a cancelled query is still running.
        self.driver.abort();
    }
}

pub(crate) struct PostgresLease {
    connection: Option<Arc<PoolConnection>>,
    permit: Option<OwnedSemaphorePermit>,
    pool: Weak<PoolInner>,
    runtime: Handle,
}

impl PostgresPool {
    pub(crate) fn new(config: Arc<Config>, options: PostgresPoolOptions) -> Self {
        Self(Arc::new(PoolInner {
            config,
            options,
            idle: Mutex::new(Vec::new()),
            permits: Arc::new(Semaphore::new(options.max_connections)),
            recycling: AtomicUsize::new(0),
            returned: Notify::new(),
        }))
    }

    pub(crate) async fn acquire(&self) -> Result<PostgresLease, ActiveRecordError> {
        tokio::time::timeout(self.0.options.acquire_timeout, self.acquire_inner())
            .await
            .map_err(|_| pool_error("acquire transaction connection", "pool checkout timed out"))?
    }

    async fn acquire_inner(&self) -> Result<PostgresLease, ActiveRecordError> {
        let permit = self
            .0
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| pool_error("acquire transaction connection", "pool is closed"))?;
        let connection = loop {
            // Register before checking recycling to avoid missing its last wake-up.
            let returned = self.0.returned.notified();
            tokio::pin!(returned);
            returned.as_mut().enable();
            let idle = self
                .0
                .idle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pop();
            if let Some(connection) = idle {
                if !connection.client.is_closed() {
                    break connection;
                }
                continue;
            }
            if self.0.recycling.load(Ordering::Acquire) == 0 {
                let (client, driver) = connect_postgres_client_with_driver(&self.0.config).await?;
                break Arc::new(PoolConnection {
                    client: Arc::new(client),
                    driver,
                });
            }
            // Prefer an already-returning connection over spawning another backend.
            returned.await;
        };
        Ok(PostgresLease {
            connection: Some(connection),
            permit: Some(permit),
            pool: Arc::downgrade(&self.0),
            runtime: Handle::current(),
        })
    }
}

impl PostgresLease {
    pub(crate) fn client(&self) -> &Arc<Client> {
        &self.connection.as_ref().expect("live lease").client
    }
}

impl Drop for PostgresLease {
    fn drop(&mut self) {
        let Some(connection) = self.connection.take() else {
            return;
        };
        let Some(pool) = self.pool.upgrade() else {
            return;
        };
        pool.recycling.fetch_add(1, Ordering::AcqRel);
        let recycle = Recycle {
            connection: Some(connection),
            permit: self.permit.take(),
            pool,
            clean: false,
        };
        self.runtime.spawn(async move {
            let mut recycle = recycle;
            let client = &recycle.connection.as_ref().expect("recycling lease").client;
            let reset = async {
                // Separate protocol requests: DISCARD ALL must be outside a transaction.
                client.batch_execute("rollback").await?;
                client.batch_execute("discard all").await
            };
            recycle.clean = matches!(
                tokio::time::timeout(recycle.pool.options.recycle_timeout, reset).await,
                Ok(Ok(()))
            ) && !client.is_closed();
            // Recycle::drop handles success, timeout, panic, and runtime shutdown.
        });
    }
}

struct Recycle {
    connection: Option<Arc<PoolConnection>>,
    permit: Option<OwnedSemaphorePermit>,
    pool: Arc<PoolInner>,
    clean: bool,
}

impl Drop for Recycle {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            if self.clean {
                self.pool
                    .idle
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(connection);
            }
            // An unclean connection is dropped/aborted before capacity is released.
        }
        self.pool.recycling.fetch_sub(1, Ordering::AcqRel);
        drop(self.permit.take());
        self.pool.returned.notify_waiters();
    }
}

fn pool_error(operation: &'static str, message: &'static str) -> ActiveRecordError {
    ActiveRecordError::Database {
        operation,
        message: message.to_string(),
    }
}
