use std::fmt;
use std::sync::{Arc, Weak};

use ricochet_application::SecretName;

use crate::session::SecretSessionInner;

const HOST_TOKEN_BYTES: usize = 32;

trait TokenGenerator: Send + Sync {
    fn fill(&self, output: &mut [u8; HOST_TOKEN_BYTES]) -> Result<(), HostTokenError>;
}

struct SystemTokenGenerator;

#[cfg(feature = "test-host")]
struct DeterministicTokenGenerator {
    seed: [u8; HOST_TOKEN_BYTES],
    counter: std::sync::atomic::AtomicU64,
}

#[derive(Clone)]
pub struct HostTokenSource {
    generator: Arc<dyn TokenGenerator>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HostTokenError;

#[derive(Clone, PartialEq, Eq)]
pub struct SecurityDomainId(pub(crate) [u8; HOST_TOKEN_BYTES]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct SecretSessionId(pub(crate) [u8; HOST_TOKEN_BYTES]);

#[derive(Clone)]
pub struct SecretRef {
    pub(crate) session: Weak<SecretSessionInner>,
    pub(crate) session_id: SecretSessionId,
    pub(crate) security_domain_id: SecurityDomainId,
    pub(crate) name: SecretName,
    pub(crate) generation: u64,
}

impl HostTokenSource {
    pub fn system() -> Self {
        Self {
            generator: Arc::new(SystemTokenGenerator),
        }
    }

    pub fn next_token(&self) -> Result<[u8; HOST_TOKEN_BYTES], HostTokenError> {
        let mut token = [0_u8; HOST_TOKEN_BYTES];
        self.generator.fill(&mut token)?;
        Ok(token)
    }

    #[cfg(feature = "test-host")]
    pub fn deterministic_for_test(seed: [u8; HOST_TOKEN_BYTES]) -> Self {
        Self {
            generator: Arc::new(DeterministicTokenGenerator {
                seed,
                counter: std::sync::atomic::AtomicU64::new(0),
            }),
        }
    }
}

impl Default for HostTokenSource {
    fn default() -> Self {
        Self::system()
    }
}

impl TokenGenerator for SystemTokenGenerator {
    fn fill(&self, output: &mut [u8; HOST_TOKEN_BYTES]) -> Result<(), HostTokenError> {
        getrandom::fill(output).map_err(|_| HostTokenError)
    }
}

#[cfg(feature = "test-host")]
impl TokenGenerator for DeterministicTokenGenerator {
    fn fill(&self, output: &mut [u8; HOST_TOKEN_BYTES]) -> Result<(), HostTokenError> {
        *output = self.seed;
        let counter = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .wrapping_add(1)
            .to_le_bytes();
        for (target, counter_byte) in output[HOST_TOKEN_BYTES - counter.len()..]
            .iter_mut()
            .zip(counter)
        {
            *target ^= counter_byte;
        }
        Ok(())
    }
}

impl SecurityDomainId {
    pub fn generate(tokens: &HostTokenSource) -> Result<Self, HostTokenError> {
        tokens.next_token().map(Self)
    }
}

impl SecretSessionId {
    pub(crate) fn generate(tokens: &HostTokenSource) -> Result<Self, HostTokenError> {
        tokens.next_token().map(Self)
    }
}

impl SecretRef {
    pub(crate) fn new(
        session: &Arc<SecretSessionInner>,
        name: SecretName,
        generation: u64,
    ) -> Self {
        Self {
            session: Arc::downgrade(session),
            session_id: session.id,
            security_domain_id: session.security_domain_id.clone(),
            name,
            generation,
        }
    }
}

impl PartialEq for SecretRef {
    fn eq(&self, other: &Self) -> bool {
        self.session_id == other.session_id
            && self.security_domain_id == other.security_domain_id
            && self.name == other.name
            && self.generation == other.generation
            && Weak::ptr_eq(&self.session, &other.session)
    }
}

impl Eq for SecretRef {}

impl fmt::Debug for HostTokenSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<host-token-source>")
    }
}

impl fmt::Debug for HostTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HostTokenError")
    }
}

impl fmt::Display for HostTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("host token generation failed")
    }
}

impl std::error::Error for HostTokenError {}

impl fmt::Debug for SecurityDomainId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<security-domain-id>")
    }
}

impl fmt::Debug for SecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secret-ref>")
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secret-ref>")
    }
}
