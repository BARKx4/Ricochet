use std::fmt;
use std::sync::Arc;

use ricochet_application::SecretName;
use zeroize::Zeroizing;

pub struct DeferredSecretSource(DeferredSecretSourceValue);

enum DeferredSecretSourceValue {
    Environment { name: SecretName },
    Literal { value: Zeroizing<String> },
}

pub struct DeferredSecretSourceError;

pub struct DeferredHttpCredentials(Arc<DeferredHttpCredentialsStorage>);

struct DeferredHttpCredentialsStorage {
    credentials: [DeferredHttpCredential; 1],
}

enum DeferredHttpCredential {
    Bearer(DeferredSecretSource),
}

impl DeferredSecretSource {
    pub fn environment(name: SecretName) -> Self {
        Self(DeferredSecretSourceValue::Environment { name })
    }

    pub fn literal(value: String) -> Result<Self, DeferredSecretSourceError> {
        if value.is_empty() {
            return Err(DeferredSecretSourceError);
        }
        Ok(Self(DeferredSecretSourceValue::Literal {
            value: Zeroizing::new(value),
        }))
    }
}

impl fmt::Debug for DeferredSecretSourceValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Environment { name } => {
                let _ = name;
                formatter.write_str("<environment-secret-source>")
            }
            Self::Literal { value } => {
                let _ = value;
                formatter.write_str("<literal-secret-source>")
            }
        }
    }
}

impl fmt::Display for DeferredSecretSourceValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl fmt::Debug for DeferredSecretSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = &self.0;
        formatter.write_str("<deferred-secret-source>")
    }
}

impl fmt::Display for DeferredSecretSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<deferred-secret-source>")
    }
}

impl fmt::Debug for DeferredSecretSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DeferredSecretSourceError")
    }
}

impl fmt::Display for DeferredSecretSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("secret literal must not be empty")
    }
}

impl std::error::Error for DeferredSecretSourceError {}

impl DeferredHttpCredentials {
    pub fn bearer(source: DeferredSecretSource) -> Self {
        Self(Arc::new(DeferredHttpCredentialsStorage {
            credentials: [DeferredHttpCredential::Bearer(source)],
        }))
    }
}

impl fmt::Debug for DeferredHttpCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bearer(source) => {
                let _ = source;
                formatter.write_str("<bearer-http-credential>")
            }
        }
    }
}

impl fmt::Display for DeferredHttpCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl fmt::Debug for DeferredHttpCredentialsStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = &self.credentials;
        formatter.write_str("<http-credentials-storage>")
    }
}

impl fmt::Display for DeferredHttpCredentialsStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl Clone for DeferredHttpCredentials {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl PartialEq for DeferredHttpCredentials {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for DeferredHttpCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<http-credentials>")
    }
}

impl fmt::Display for DeferredHttpCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<http-credentials>")
    }
}

#[cfg(test)]
mod tests {
    use ricochet_application::SecretName;

    use super::*;

    #[test]
    fn deferred_secret_sources_use_strict_constructors_and_redacted_formatting() {
        let environment = DeferredSecretSource::environment(
            SecretName::parse("provider.api-key").expect("fixture name should parse"),
        );
        let literal = DeferredSecretSource::literal("synthetic-secret-value".to_string())
            .expect("non-empty literal should be accepted");

        match &literal.0 {
            DeferredSecretSourceValue::Literal { value } => {
                let _: &zeroize::Zeroizing<String> = value;
                assert_eq!(value.as_str(), "synthetic-secret-value");
            }
            DeferredSecretSourceValue::Environment { .. } => {
                panic!("literal constructor should store a literal source")
            }
        }

        assert!(!format!("{environment}").contains("provider"));
        assert!(!format!("{environment:?}").contains("provider"));
        assert!(!format!("{literal}").contains("synthetic"));
        assert!(!format!("{literal:?}").contains("synthetic"));
        assert!(DeferredSecretSource::literal(String::new()).is_err());
    }

    #[test]
    fn deferred_http_credentials_are_shared_opaque_and_exactly_formatted() {
        let source = DeferredSecretSource::literal("synthetic-secret-value".to_string())
            .expect("non-empty literal should be accepted");
        let credentials = DeferredHttpCredentials::bearer(source);
        let clone = credentials.clone();
        let separate = DeferredHttpCredentials::bearer(
            DeferredSecretSource::literal("synthetic-secret-value".to_string())
                .expect("non-empty literal should be accepted"),
        );

        assert_eq!(format!("{credentials}"), "<http-credentials>");
        assert_eq!(format!("{credentials:?}"), "<http-credentials>");
        assert_eq!(credentials, clone, "clones should share opaque storage");
        assert_ne!(
            credentials, separate,
            "separate credentials should compare by storage identity"
        );
    }
}
