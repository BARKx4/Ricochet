use serde::{Deserialize, Serialize};

use crate::error::{DiagnosticMetadata, FailedGuarantee, SandboxError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Preparing,
    Ready,
    Running,
    Stopping,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionLifecycle {
    state: SessionState,
}

impl SessionLifecycle {
    pub fn new() -> Self {
        Self {
            state: SessionState::Preparing,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    #[allow(clippy::result_large_err)]
    pub fn transition(&mut self, next: SessionState) -> Result<(), SandboxError> {
        let allowed = matches!(
            (self.state, next),
            (
                SessionState::Preparing,
                SessionState::Ready | SessionState::Failed
            ) | (
                SessionState::Ready,
                SessionState::Running | SessionState::Stopping | SessionState::Failed
            ) | (
                SessionState::Running,
                SessionState::Stopping | SessionState::Failed
            ) | (
                SessionState::Stopping,
                SessionState::Closed | SessionState::Failed
            )
        );
        if !allowed {
            return Err(SandboxError::policy(
                FailedGuarantee::SessionOwnership,
                DiagnosticMetadata::empty(),
            ));
        }

        self.state = next;
        Ok(())
    }
}

impl Default for SessionLifecycle {
    fn default() -> Self {
        Self::new()
    }
}
