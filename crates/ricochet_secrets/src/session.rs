use std::collections::BTreeMap;
use std::fmt;
#[cfg(feature = "test-host")]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ricochet_application::SecretName;
use zeroize::Zeroizing;

use crate::opaque::{
    HostTokenError, HostTokenSource, SecretRef, SecretSessionId, SecurityDomainId,
};

const MAX_SESSION_SLOTS: usize = 32;
const MAX_SECRET_BYTES: usize = 2048;

pub(crate) struct SecretSessionInner {
    pub(crate) id: SecretSessionId,
    pub(crate) security_domain_id: SecurityDomainId,
    closed: AtomicBool,
    state: Mutex<SecretSessionState>,
    #[cfg(feature = "test-host")]
    resolution_count: AtomicUsize,
}

#[derive(Default)]
struct SecretSessionState {
    slots: BTreeMap<SecretName, SecretSlot>,
}

struct SecretSlot {
    generation: u64,
    value: Zeroizing<String>,
}

#[derive(Clone)]
pub struct SecretSession {
    inner: Arc<SecretSessionInner>,
}

#[derive(Clone)]
pub struct SecretSessionContext {
    inner: Arc<SecretSessionInner>,
}

#[derive(Clone)]
pub struct SecretSessionGuard {
    inner: Arc<SecretSessionInner>,
}

pub struct SessionSecretPrompt {
    inner: Arc<SecretSessionInner>,
    name: SecretName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretSessionErrorKind {
    Closed,
    Missing,
    Stale,
    WrongSession,
    WrongSecurityDomain,
    InvalidValue,
    Capacity,
    GenerationExhausted,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SecretSessionError {
    kind: SecretSessionErrorKind,
}

impl SecretSession {
    pub fn create(
        tokens: &HostTokenSource,
        security_domain_id: SecurityDomainId,
    ) -> Result<(Self, SecretSessionGuard), HostTokenError> {
        let inner = Arc::new(SecretSessionInner {
            id: SecretSessionId::generate(tokens)?,
            security_domain_id,
            closed: AtomicBool::new(false),
            state: Mutex::new(SecretSessionState::default()),
            #[cfg(feature = "test-host")]
            resolution_count: AtomicUsize::new(0),
        });
        Ok((
            Self {
                inner: Arc::clone(&inner),
            },
            SecretSessionGuard { inner },
        ))
    }

    pub fn context(&self) -> SecretSessionContext {
        SecretSessionContext {
            inner: Arc::clone(&self.inner),
        }
    }

    #[cfg(feature = "test-host")]
    pub fn test_slot_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .expect("secret session lock poisoned")
            .slots
            .len()
    }

    #[cfg(feature = "test-host")]
    pub fn test_resolution_count(&self) -> usize {
        self.inner.resolution_count.load(Ordering::Acquire)
    }
}

impl SecretSessionContext {
    pub fn prompt(&self, name: SecretName) -> Result<SessionSecretPrompt, SecretSessionError> {
        ensure_open(&self.inner)?;
        Ok(SessionSecretPrompt {
            inner: Arc::clone(&self.inner),
            name,
        })
    }

    pub fn present(&self, name: &SecretName) -> Result<bool, SecretSessionError> {
        ensure_open(&self.inner)?;
        let state = self
            .inner
            .state
            .lock()
            .expect("secret session lock poisoned");
        ensure_open(&self.inner)?;
        Ok(state.slots.contains_key(name))
    }

    pub fn reference(&self, name: &SecretName) -> Result<SecretRef, SecretSessionError> {
        ensure_open(&self.inner)?;
        let state = self
            .inner
            .state
            .lock()
            .expect("secret session lock poisoned");
        ensure_open(&self.inner)?;
        let slot = state
            .slots
            .get(name)
            .ok_or_else(|| SecretSessionError::new(SecretSessionErrorKind::Missing))?;
        Ok(SecretRef::new(&self.inner, name.clone(), slot.generation))
    }

    pub(crate) fn validate_reference(
        &self,
        reference: &SecretRef,
        current_domain: &SecurityDomainId,
    ) -> Result<(), SecretSessionError> {
        ensure_reference_authority(&self.inner, reference, current_domain)?;
        let state = self
            .inner
            .state
            .lock()
            .expect("secret session lock poisoned");
        ensure_open(&self.inner)?;
        let slot = state
            .slots
            .get(&reference.name)
            .ok_or_else(|| SecretSessionError::new(SecretSessionErrorKind::Missing))?;
        if slot.generation != reference.generation {
            return Err(SecretSessionError::new(SecretSessionErrorKind::Stale));
        }
        Ok(())
    }

    pub(crate) fn resolve_reference(
        &self,
        reference: &SecretRef,
        current_domain: &SecurityDomainId,
    ) -> Result<Zeroizing<String>, SecretSessionError> {
        #[cfg(feature = "test-host")]
        self.inner.resolution_count.fetch_add(1, Ordering::AcqRel);
        ensure_reference_authority(&self.inner, reference, current_domain)?;
        let state = self
            .inner
            .state
            .lock()
            .expect("secret session lock poisoned");
        ensure_open(&self.inner)?;
        let slot = state
            .slots
            .get(&reference.name)
            .ok_or_else(|| SecretSessionError::new(SecretSessionErrorKind::Missing))?;
        if slot.generation != reference.generation {
            return Err(SecretSessionError::new(SecretSessionErrorKind::Stale));
        }
        Ok(Zeroizing::new(slot.value.to_string()))
    }
}

impl SessionSecretPrompt {
    pub fn bind(self, value: Zeroizing<String>) -> Result<SecretRef, SecretSessionError> {
        if value.is_empty() || value.len() > MAX_SECRET_BYTES {
            return Err(SecretSessionError::new(
                SecretSessionErrorKind::InvalidValue,
            ));
        }
        ensure_open(&self.inner)?;
        let mut state = self
            .inner
            .state
            .lock()
            .expect("secret session lock poisoned");
        ensure_open(&self.inner)?;
        let generation = match state.slots.get(&self.name) {
            Some(existing) => existing.generation.checked_add(1).ok_or_else(|| {
                SecretSessionError::new(SecretSessionErrorKind::GenerationExhausted)
            })?,
            None if state.slots.len() >= MAX_SESSION_SLOTS => {
                return Err(SecretSessionError::new(SecretSessionErrorKind::Capacity));
            }
            None => 1,
        };
        state
            .slots
            .insert(self.name.clone(), SecretSlot { generation, value });
        Ok(SecretRef::new(&self.inner, self.name, generation))
    }
}

impl SecretSessionGuard {
    pub fn close(&self) {
        if self.inner.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.inner
            .state
            .lock()
            .expect("secret session lock poisoned")
            .slots
            .clear();
    }
}

impl Drop for SecretSessionGuard {
    fn drop(&mut self) {
        self.close();
    }
}

impl SecretSessionError {
    fn new(kind: SecretSessionErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> SecretSessionErrorKind {
        self.kind
    }
}

fn ensure_open(inner: &SecretSessionInner) -> Result<(), SecretSessionError> {
    if inner.closed.load(Ordering::Acquire) {
        Err(SecretSessionError::new(SecretSessionErrorKind::Closed))
    } else {
        Ok(())
    }
}

fn ensure_reference_authority(
    inner: &Arc<SecretSessionInner>,
    reference: &SecretRef,
    current_domain: &SecurityDomainId,
) -> Result<(), SecretSessionError> {
    ensure_open(inner)?;
    if reference.session_id != inner.id
        || reference
            .session
            .upgrade()
            .is_none_or(|session| !Arc::ptr_eq(&session, inner))
    {
        return Err(SecretSessionError::new(
            SecretSessionErrorKind::WrongSession,
        ));
    }
    if &inner.security_domain_id != current_domain
        || &reference.security_domain_id != current_domain
    {
        return Err(SecretSessionError::new(
            SecretSessionErrorKind::WrongSecurityDomain,
        ));
    }
    Ok(())
}

impl fmt::Debug for SecretSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secret-session>")
    }
}

impl fmt::Debug for SecretSessionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secret-session-context>")
    }
}

impl fmt::Debug for SecretSessionGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secret-session-guard>")
    }
}

impl fmt::Debug for SessionSecretPrompt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<session-secret-prompt>")
    }
}

impl fmt::Debug for SecretSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretSessionError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl fmt::Display for SecretSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            SecretSessionErrorKind::Closed => "secret session is closed",
            SecretSessionErrorKind::Missing => "secret_missing",
            SecretSessionErrorKind::Stale => "secret reference is stale",
            SecretSessionErrorKind::WrongSession => "secret reference session mismatch",
            SecretSessionErrorKind::WrongSecurityDomain => {
                "secret reference security domain mismatch"
            }
            SecretSessionErrorKind::InvalidValue => {
                "session secret must contain 1 to 2048 UTF-8 bytes"
            }
            SecretSessionErrorKind::Capacity => "secret session capacity exceeded",
            SecretSessionErrorKind::GenerationExhausted => "secret slot generation exhausted",
        })
    }
}

impl std::error::Error for SecretSessionError {}
