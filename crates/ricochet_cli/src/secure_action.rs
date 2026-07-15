use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ricochet_secrets::HostTokenSource;

const ACTION_LIFETIME: Duration = Duration::from_secs(5 * 60);
const MAX_ACTIONS_PER_DOCUMENT: usize = 32;

pub trait MonotonicClock: Send + Sync {
    fn now(&self) -> Duration;
}

struct SystemMonotonicClock {
    epoch: Instant,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SecretActionId(String);

#[derive(Clone)]
pub struct SecretActionRegistry<T> {
    inner: Arc<SecretActionRegistryInner<T>>,
}

struct SecretActionRegistryInner<T> {
    tokens: HostTokenSource,
    clock: Arc<dyn MonotonicClock>,
    entries: Mutex<BTreeMap<SecretActionId, SecretActionEntry<T>>>,
}

struct SecretActionEntry<T> {
    generation: u64,
    issued_at: Duration,
    binding: T,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretActionErrorKind {
    Missing,
    WrongGeneration,
    Expired,
    Capacity,
    Token,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SecretActionError {
    kind: SecretActionErrorKind,
}

impl<T> SecretActionRegistry<T> {
    pub fn new(tokens: HostTokenSource) -> Self {
        Self::with_clock(
            tokens,
            Arc::new(SystemMonotonicClock {
                epoch: Instant::now(),
            }),
        )
    }

    fn with_clock(tokens: HostTokenSource, clock: Arc<dyn MonotonicClock>) -> Self {
        Self {
            inner: Arc::new(SecretActionRegistryInner {
                tokens,
                clock,
                entries: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    pub fn issue(&self, generation: u64, binding: T) -> Result<SecretActionId, SecretActionError> {
        let now = self.inner.clock.now();
        let mut entries = self
            .inner
            .entries
            .lock()
            .expect("secure action registry lock poisoned");
        entries.retain(|_, entry| now.saturating_sub(entry.issued_at) < ACTION_LIFETIME);
        if entries
            .values()
            .filter(|entry| entry.generation == generation)
            .count()
            >= MAX_ACTIONS_PER_DOCUMENT
        {
            return Err(SecretActionError::new(SecretActionErrorKind::Capacity));
        }
        for _ in 0..16 {
            let token = self
                .inner
                .tokens
                .next_token()
                .map_err(|_| SecretActionError::new(SecretActionErrorKind::Token))?;
            let id = SecretActionId(hex_token(token));
            if entries.contains_key(&id) {
                continue;
            }
            entries.insert(
                id.clone(),
                SecretActionEntry {
                    generation,
                    issued_at: now,
                    binding,
                },
            );
            return Ok(id);
        }
        Err(SecretActionError::new(SecretActionErrorKind::Token))
    }

    pub fn take(&self, id: &SecretActionId, generation: u64) -> Result<T, SecretActionError> {
        let now = self.inner.clock.now();
        let mut entries = self
            .inner
            .entries
            .lock()
            .expect("secure action registry lock poisoned");
        let entry = entries
            .get(id)
            .ok_or_else(|| SecretActionError::new(SecretActionErrorKind::Missing))?;
        if entry.generation != generation {
            return Err(SecretActionError::new(
                SecretActionErrorKind::WrongGeneration,
            ));
        }
        if now.saturating_sub(entry.issued_at) >= ACTION_LIFETIME {
            entries.remove(id);
            return Err(SecretActionError::new(SecretActionErrorKind::Expired));
        }
        entries
            .remove(id)
            .map(|entry| entry.binding)
            .ok_or_else(|| SecretActionError::new(SecretActionErrorKind::Missing))
    }

    pub fn invalidate_generation(&self, generation: u64) {
        self.inner
            .entries
            .lock()
            .expect("secure action registry lock poisoned")
            .retain(|_, entry| entry.generation != generation);
    }

    pub fn invalidate_all(&self) {
        self.inner
            .entries
            .lock()
            .expect("secure action registry lock poisoned")
            .clear();
    }
}

impl SecretActionId {
    pub fn parse(value: &str) -> Result<Self, SecretActionError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(SecretActionError::new(SecretActionErrorKind::Missing));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SecretActionError {
    fn new(kind: SecretActionErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> SecretActionErrorKind {
        self.kind
    }
}

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Duration {
        self.epoch.elapsed()
    }
}

fn hex_token(token: [u8; 32]) -> String {
    use fmt::Write as _;

    let mut output = String::with_capacity(64);
    for byte in token {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

impl fmt::Debug for SecretActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secret-action-id>")
    }
}

impl fmt::Debug for SecretActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretActionError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl fmt::Display for SecretActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("secure action is unavailable")
    }
}

impl std::error::Error for SecretActionError {}

#[cfg(test)]
#[derive(Clone)]
pub struct TestMonotonicClock {
    now: Arc<Mutex<Duration>>,
}

#[cfg(test)]
impl TestMonotonicClock {
    pub fn new(now: Duration) -> Self {
        Self {
            now: Arc::new(Mutex::new(now)),
        }
    }

    pub fn advance(&self, duration: Duration) {
        let mut now = self.now.lock().expect("test clock lock poisoned");
        *now = now.saturating_add(duration);
    }
}

#[cfg(test)]
impl MonotonicClock for TestMonotonicClock {
    fn now(&self) -> Duration {
        *self.now.lock().expect("test clock lock poisoned")
    }
}

#[cfg(test)]
impl<T> SecretActionRegistry<T> {
    pub fn with_test_host(tokens: HostTokenSource, clock: TestMonotonicClock) -> Self {
        Self::with_clock(tokens, Arc::new(clock))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;

    #[test]
    fn secure_session_action_is_generation_bound_one_use_and_exactly_five_minutes() {
        let tokens = HostTokenSource::deterministic_for_test([0x55; 32]);
        let clock = TestMonotonicClock::new(Duration::ZERO);
        let registry = SecretActionRegistry::with_test_host(tokens, clock.clone());
        let id = registry.issue(7, "binding").expect("action issue");
        assert_eq!(
            registry.take(&id, 8).expect_err("wrong generation").kind(),
            SecretActionErrorKind::WrongGeneration
        );
        assert_eq!(registry.take(&id, 7).expect("one use"), "binding");
        assert_eq!(
            registry.take(&id, 7).expect_err("replay").kind(),
            SecretActionErrorKind::Missing
        );
        let expires = registry.issue(7, "expires").expect("expiring action");
        clock.advance(ACTION_LIFETIME);
        assert_eq!(
            registry.take(&expires, 7).expect_err("expiry").kind(),
            SecretActionErrorKind::Expired
        );
    }

    #[test]
    fn secure_session_action_capacity_navigation_and_double_click_are_atomic() {
        let tokens = HostTokenSource::deterministic_for_test([0x56; 32]);
        let clock = TestMonotonicClock::new(Duration::ZERO);
        let registry = Arc::new(SecretActionRegistry::with_test_host(tokens, clock));
        let mut ids = (0..32)
            .map(|index| registry.issue(11, index).expect("bounded action"))
            .collect::<Vec<_>>();
        assert_eq!(
            registry.issue(11, 33).expect_err("capacity").kind(),
            SecretActionErrorKind::Capacity
        );
        let double_click = ids.pop().expect("double-click action");
        let barrier = Arc::new(Barrier::new(3));
        let workers = (0..2)
            .map(|_| {
                let registry = Arc::clone(&registry);
                let id = double_click.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    registry.take(&id, 11)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| worker.join().expect("worker"))
                .filter(Result::is_ok)
                .count(),
            1
        );
        registry.invalidate_generation(11);
        for id in ids {
            assert_eq!(
                registry.take(&id, 11).expect_err("invalidated").kind(),
                SecretActionErrorKind::Missing
            );
        }
    }
}
