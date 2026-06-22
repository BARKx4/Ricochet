use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::value::Value;

const MAX_RETAINED_APPROVAL_RECORDS: usize = 1024;

#[derive(Clone)]
pub struct ApprovalRegistry {
    inner: Arc<Mutex<ApprovalRegistryState>>,
}

struct ApprovalRegistryState {
    next_id: u64,
    max_retained: usize,
    approvals: BTreeMap<String, ApprovalRecord>,
}

#[derive(Clone, Debug, PartialEq)]
struct ApprovalRecord {
    id: String,
    token: String,
    operation: Value,
    metadata: Value,
    status: ApprovalStatus,
    created_at_ms: i64,
    expires_at_ms: Option<i64>,
    claimed_at_ms: Option<i64>,
    completed_at_ms: Option<i64>,
    rejected_at_ms: Option<i64>,
    completed_result: Option<Value>,
    rejection_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ApprovalStatus {
    Pending,
    Claimed,
    Completed,
    Rejected,
    Expired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalCreateRequest {
    pub id: Option<String>,
    pub token: Option<String>,
    pub operation: Value,
    pub metadata: Value,
    pub ttl_ms: Option<i64>,
    pub expires_at_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalSnapshot {
    pub id: String,
    pub token: Option<String>,
    pub operation: Value,
    pub metadata: Value,
    pub status: String,
    pub pending: bool,
    pub claimed: bool,
    pub completed: bool,
    pub rejected: bool,
    pub expired: bool,
    pub created_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    pub claimed_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub rejected_at_ms: Option<i64>,
    pub completed_result: Option<Value>,
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRuntimeError {
    pub kind: &'static str,
    pub message: String,
}

impl ApprovalRuntimeError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl ApprovalRegistry {
    pub fn new() -> Self {
        Self::with_max_retained(MAX_RETAINED_APPROVAL_RECORDS)
    }

    pub fn with_max_retained(max_retained: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ApprovalRegistryState {
                next_id: 0,
                max_retained,
                approvals: BTreeMap::new(),
            })),
        }
    }

    pub fn create(
        &self,
        request: ApprovalCreateRequest,
    ) -> Result<ApprovalSnapshot, ApprovalRuntimeError> {
        let now = now_ms();
        let mut state = self.inner.lock().expect("approval registry lock poisoned");
        state.prune_releasable(now);
        if state.approvals.len() >= state.max_retained {
            return Err(ApprovalRuntimeError::new(
                "ApprovalRegistryFull",
                format!(
                    "approval registry reached its retained record limit of {}",
                    state.max_retained
                ),
            ));
        }
        let id = match request.id {
            Some(id) => id,
            None => {
                state.next_id += 1;
                format!("approval_{}_{}", now, state.next_id)
            }
        };
        if state.approvals.contains_key(&id) {
            return Err(ApprovalRuntimeError::new(
                "ApprovalAlreadyExists",
                format!("approval already exists: {id}"),
            ));
        }

        let token = match request.token {
            Some(token) => token,
            None => random_token()?,
        };
        let expires_at_ms = request
            .expires_at_ms
            .or_else(|| request.ttl_ms.map(|ttl| now.saturating_add(ttl)));
        let record = ApprovalRecord {
            id: id.clone(),
            token,
            operation: request.operation,
            metadata: request.metadata,
            status: ApprovalStatus::Pending,
            created_at_ms: now,
            expires_at_ms,
            claimed_at_ms: None,
            completed_at_ms: None,
            rejected_at_ms: None,
            completed_result: None,
            rejection_reason: None,
        };
        let snapshot = record.snapshot(true);
        state.approvals.insert(id, record);
        Ok(snapshot)
    }

    pub fn claim(&self, id: &str, token: &str) -> Result<ApprovalSnapshot, ApprovalRuntimeError> {
        let now = now_ms();
        let mut state = self.inner.lock().expect("approval registry lock poisoned");
        let record = state
            .approvals
            .get_mut(id)
            .ok_or_else(|| approval_not_found(id))?;
        record.expire_if_needed(now);
        match record.status {
            ApprovalStatus::Pending => {}
            ApprovalStatus::Claimed => {
                return Err(ApprovalRuntimeError::new(
                    "ApprovalAlreadyClaimed",
                    format!("approval was already claimed: {id}"),
                ));
            }
            ApprovalStatus::Completed | ApprovalStatus::Rejected => {
                return Err(ApprovalRuntimeError::new(
                    "ApprovalFinal",
                    format!("approval is already final: {id}"),
                ));
            }
            ApprovalStatus::Expired => {
                return Err(ApprovalRuntimeError::new(
                    "ApprovalExpired",
                    format!("approval has expired: {id}"),
                ));
            }
        }
        if record.token != token {
            return Err(ApprovalRuntimeError::new(
                "ApprovalDenied",
                "approval token did not match",
            ));
        }
        record.status = ApprovalStatus::Claimed;
        record.claimed_at_ms = Some(now);
        Ok(record.snapshot(false))
    }

    pub fn complete(
        &self,
        id: &str,
        result: Value,
    ) -> Result<ApprovalSnapshot, ApprovalRuntimeError> {
        let now = now_ms();
        let mut state = self.inner.lock().expect("approval registry lock poisoned");
        let record = state
            .approvals
            .get_mut(id)
            .ok_or_else(|| approval_not_found(id))?;
        record.expire_if_needed(now);
        match record.status {
            ApprovalStatus::Claimed => {}
            ApprovalStatus::Pending => {
                return Err(ApprovalRuntimeError::new(
                    "ApprovalNotClaimed",
                    format!("approval must be claimed before completion: {id}"),
                ));
            }
            ApprovalStatus::Completed | ApprovalStatus::Rejected => {
                return Err(ApprovalRuntimeError::new(
                    "ApprovalFinal",
                    format!("approval is already final: {id}"),
                ));
            }
            ApprovalStatus::Expired => {
                return Err(ApprovalRuntimeError::new(
                    "ApprovalExpired",
                    format!("approval has expired: {id}"),
                ));
            }
        }
        record.status = ApprovalStatus::Completed;
        record.completed_at_ms = Some(now);
        record.completed_result = Some(result);
        Ok(record.snapshot(false))
    }

    pub fn reject(
        &self,
        id: &str,
        reason: String,
    ) -> Result<ApprovalSnapshot, ApprovalRuntimeError> {
        let now = now_ms();
        let mut state = self.inner.lock().expect("approval registry lock poisoned");
        let record = state
            .approvals
            .get_mut(id)
            .ok_or_else(|| approval_not_found(id))?;
        record.expire_if_needed(now);
        match record.status {
            ApprovalStatus::Pending | ApprovalStatus::Claimed => {}
            ApprovalStatus::Completed | ApprovalStatus::Rejected => {
                return Err(ApprovalRuntimeError::new(
                    "ApprovalFinal",
                    format!("approval is already final: {id}"),
                ));
            }
            ApprovalStatus::Expired => {
                return Err(ApprovalRuntimeError::new(
                    "ApprovalExpired",
                    format!("approval has expired: {id}"),
                ));
            }
        }
        record.status = ApprovalStatus::Rejected;
        record.rejected_at_ms = Some(now);
        record.rejection_reason = Some(reason);
        Ok(record.snapshot(false))
    }

    pub fn detail(&self, id: &str) -> Result<ApprovalSnapshot, ApprovalRuntimeError> {
        let now = now_ms();
        let mut state = self.inner.lock().expect("approval registry lock poisoned");
        let record = state
            .approvals
            .get_mut(id)
            .ok_or_else(|| approval_not_found(id))?;
        record.expire_if_needed(now);
        Ok(record.snapshot(false))
    }

    pub fn release(&self, id: &str) -> Result<bool, ApprovalRuntimeError> {
        let mut state = self.inner.lock().expect("approval registry lock poisoned");
        Ok(state.approvals.remove(id).is_some())
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("approval registry lock poisoned")
            .approvals
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("approval registry lock poisoned")
            .approvals
            .is_empty()
    }
}

impl Default for ApprovalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalRegistryState {
    fn prune_releasable(&mut self, now: i64) {
        for record in self.approvals.values_mut() {
            record.expire_if_needed(now);
        }
        while self.approvals.len() >= self.max_retained {
            let Some(key) = self
                .approvals
                .iter()
                .filter(|(_, record)| record.is_releasable())
                .min_by_key(|(_, record)| record.finalized_at_ms().unwrap_or(record.created_at_ms))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.approvals.remove(&key);
        }
    }
}

impl ApprovalRecord {
    fn snapshot(&self, include_token: bool) -> ApprovalSnapshot {
        ApprovalSnapshot {
            id: self.id.clone(),
            token: include_token.then(|| self.token.clone()),
            operation: self.operation.clone(),
            metadata: self.metadata.clone(),
            status: self.status.as_str().to_string(),
            pending: self.status == ApprovalStatus::Pending,
            claimed: self.status == ApprovalStatus::Claimed,
            completed: self.status == ApprovalStatus::Completed,
            rejected: self.status == ApprovalStatus::Rejected,
            expired: self.status == ApprovalStatus::Expired,
            created_at_ms: self.created_at_ms,
            expires_at_ms: self.expires_at_ms,
            claimed_at_ms: self.claimed_at_ms,
            completed_at_ms: self.completed_at_ms,
            rejected_at_ms: self.rejected_at_ms,
            completed_result: self.completed_result.clone(),
            rejection_reason: self.rejection_reason.clone(),
        }
    }

    fn expire_if_needed(&mut self, now: i64) {
        if matches!(self.status, ApprovalStatus::Pending)
            && self
                .expires_at_ms
                .is_some_and(|expires_at| now > expires_at)
        {
            self.status = ApprovalStatus::Expired;
        }
    }

    fn is_releasable(&self) -> bool {
        matches!(
            self.status,
            ApprovalStatus::Completed | ApprovalStatus::Rejected | ApprovalStatus::Expired
        )
    }

    fn finalized_at_ms(&self) -> Option<i64> {
        match self.status {
            ApprovalStatus::Completed => self.completed_at_ms,
            ApprovalStatus::Rejected => self.rejected_at_ms,
            ApprovalStatus::Expired => self.expires_at_ms,
            ApprovalStatus::Pending | ApprovalStatus::Claimed => None,
        }
    }
}

impl ApprovalStatus {
    fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Claimed => "claimed",
            ApprovalStatus::Completed => "completed",
            ApprovalStatus::Rejected => "rejected",
            ApprovalStatus::Expired => "expired",
        }
    }
}

fn approval_not_found(id: &str) -> ApprovalRuntimeError {
    ApprovalRuntimeError::new("ApprovalNotFound", format!("unknown approval: {id}"))
}

fn random_token() -> Result<String, ApprovalRuntimeError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| ApprovalRuntimeError::new("ApprovalRandomError", error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_request(id: &str, ttl_ms: Option<i64>) -> ApprovalCreateRequest {
        ApprovalCreateRequest {
            id: Some(id.to_string()),
            token: Some("token".to_string()),
            operation: Value::Map(BTreeMap::new().into()),
            metadata: Value::Map(BTreeMap::new().into()),
            ttl_ms,
            expires_at_ms: None,
        }
    }

    #[test]
    fn approval_release_removes_retained_record() {
        let registry = ApprovalRegistry::new();
        registry
            .create(create_request("approval-one", None))
            .expect("approval should be created");

        assert_eq!(registry.len(), 1);
        assert!(registry
            .release("approval-one")
            .expect("release should work"));
        assert_eq!(registry.len(), 0);
        assert!(!registry
            .release("approval-one")
            .expect("release should work"));
    }

    #[test]
    fn approval_registry_prunes_expired_records_before_limit_error() {
        let registry = ApprovalRegistry::with_max_retained(1);
        registry
            .create(create_request("expired", Some(-1)))
            .expect("expired approval should be created");

        registry
            .create(create_request("replacement", None))
            .expect("expired approval should be pruned for a replacement");

        assert!(matches!(
            registry.detail("expired"),
            Err(ApprovalRuntimeError {
                kind: "ApprovalNotFound",
                ..
            })
        ));
        assert_eq!(registry.len(), 1);
    }
}
