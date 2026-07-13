use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct WorkspaceWriteRegistry {
    inner: Arc<Mutex<()>>,
}

impl WorkspaceWriteRegistry {
    pub(crate) fn synchronize<T>(&self, operation: impl FnOnce() -> T) -> Result<T, String> {
        let _guard = self
            .inner
            .lock()
            .map_err(|_| "workspace write registry lock poisoned".to_string())?;
        Ok(operation())
    }

    pub fn shares_state_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{mpsc, TryLockError};
    use std::thread;

    use super::WorkspaceWriteRegistry;

    #[test]
    fn workspace_write_registry_clones_serialize_operations() {
        let registry = WorkspaceWriteRegistry::default();
        let first_registry = registry.clone();
        let second_registry = registry.clone();
        let (first_entered_tx, first_entered_rx) = mpsc::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();
        let (second_started_tx, second_started_rx) = mpsc::channel();
        let (second_entered_tx, second_entered_rx) = mpsc::channel();

        let first = thread::spawn(move || {
            first_registry
                .synchronize(|| {
                    first_entered_tx.send(()).expect("signal first entry");
                    release_first_rx.recv().expect("wait for first release");
                })
                .expect("first operation should synchronize");
        });
        first_entered_rx
            .recv()
            .expect("first operation should enter");
        assert!(
            matches!(
                second_registry.inner.try_lock(),
                Err(TryLockError::WouldBlock)
            ),
            "the second clone's mutex was not locked by the first clone"
        );

        let second = thread::spawn(move || {
            second_started_tx.send(()).expect("signal second start");
            second_registry
                .synchronize(|| {
                    second_entered_tx.send(()).expect("signal second entry");
                })
                .expect("second operation should synchronize");
        });
        second_started_rx
            .recv()
            .expect("second operation should start");
        assert!(
            second_entered_rx.try_recv().is_err(),
            "second clone entered before the first clone was released"
        );

        release_first_tx.send(()).expect("release first operation");
        second_entered_rx
            .recv()
            .expect("second operation should enter after first release");
        first.join().expect("first operation thread should finish");
        second
            .join()
            .expect("second operation thread should finish");
    }
}
