use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppRevision {
    pub id: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RevisionManager {
    current: Arc<AtomicU64>,
}

impl RevisionManager {
    pub fn current(&self) -> AppRevision {
        AppRevision {
            id: self.current.load(Ordering::SeqCst),
        }
    }

    pub fn publish_new_revision(&self) -> AppRevision {
        AppRevision {
            id: self.current.fetch_add(1, Ordering::SeqCst) + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requests_get_latest_revision_while_existing_keeps_snapshot() {
        let revisions = RevisionManager::default();

        let first = revisions.current();
        assert_eq!(first.id, 0);

        let second = revisions.publish_new_revision();
        assert_eq!(second.id, 1);

        let latest = revisions.current();
        assert_eq!(latest.id, 1);
        assert_eq!(first.id, 0);
    }
}
