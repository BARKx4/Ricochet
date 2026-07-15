//! Deferred HTTP credentials remain opaque across the public crate boundary.
//!
//! Credential bytes cannot be extracted from a prepared request:
//!
//! ```compile_fail
//! use ricochet_secrets::PreparedSecretHttpRequest;
//!
//! fn leak(request: &PreparedSecretHttpRequest) -> &str {
//!     request.credentials.as_str()
//! }
//! ```
//!
//! The production executor accepts no caller-supplied resolution callback:
//!
//! ```compile_fail
//! use ricochet_secrets::SecretsHttpExecutor;
//!
//! let _ = SecretsHttpExecutor::with_resolver(|_| "plaintext-secret".to_string());
//! ```
//!
//! There is no public consumer marker that callers can forge:
//!
//! ```compile_fail
//! use ricochet_secrets::SecretHttpCredentialConsumer;
//!
//! let _ = SecretHttpCredentialConsumer::Authorization;
//! ```

mod deferred_http;
mod http_executor;

pub use deferred_http::{DeferredHttpCredentials, DeferredSecretSource, DeferredSecretSourceError};
#[cfg(feature = "test-host")]
pub use http_executor::test_host;
pub use http_executor::{
    EnvironmentCredentialPolicy, PreparedSecretHttpRequest, SecretHttpError,
    SecretHttpPolicySnapshot, SecretHttpResponse, SecretHttpResponseStream, SecretsHttpExecutor,
};
