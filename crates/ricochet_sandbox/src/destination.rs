use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{DiagnosticMetadata, FailedGuarantee, SandboxError};

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestinationGrant {
    host: String,
    port: u16,
}

impl DestinationGrant {
    #[allow(clippy::result_large_err)]
    pub fn parse(value: &str) -> Result<Self, SandboxError> {
        value.parse()
    }

    #[allow(clippy::result_large_err)]
    pub fn new(host: &str, port: u16) -> Result<Self, SandboxError> {
        if port == 0 || host.chars().any(char::is_whitespace) {
            return Err(validation_error());
        }

        let host = host.strip_suffix('.').unwrap_or(host);
        if host.is_empty() || has_uts46_deviation(host) {
            return Err(validation_error());
        }

        let host = idna::domain_to_ascii_strict(host)
            .map_err(|_| validation_error())?
            .to_ascii_lowercase();
        let (unicode, unicode_result) = idna::domain_to_unicode(&host);

        if unicode_result.is_err()
            || has_uts46_deviation(&unicode)
            || host.len() > 253
            || !host.split('.').all(valid_ldh_label)
            || host.parse::<IpAddr>().is_ok()
            || host == "localhost"
            || host.ends_with(".localhost")
        {
            return Err(validation_error());
        }

        Ok(Self { host, port })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl FromStr for DestinationGrant {
    type Err = SandboxError;

    #[allow(clippy::result_large_err)]
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.chars().any(char::is_whitespace) {
            return Err(validation_error());
        }

        let (host, port) = value.rsplit_once(':').ok_or_else(validation_error)?;
        if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(validation_error());
        }

        let port = port.parse::<u16>().map_err(|_| validation_error())?;
        Self::new(host, port)
    }
}

impl fmt::Display for DestinationGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.host, self.port)
    }
}

impl Serialize for DestinationGrant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for DestinationGrant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(D::Error::custom)
    }
}

fn validation_error() -> SandboxError {
    SandboxError::policy(
        FailedGuarantee::DestinationGrant,
        DiagnosticMetadata::empty(),
    )
}

fn has_uts46_deviation(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character, 'ß' | 'ẞ' | 'ς' | '\u{200c}' | '\u{200d}'))
}

fn valid_ldh_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && label
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && label
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && label
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::DestinationGrant;
    use crate::DiagnosticMetadata;

    #[test]
    fn diagnostic_metadata_debug_redacts_raw_destination() {
        let raw_destination = "private.internal.example:443";
        let metadata = DiagnosticMetadata::empty()
            .with_destination(DestinationGrant::parse(raw_destination).unwrap());

        let debug = format!("{metadata:?}");

        assert!(
            !debug.contains(raw_destination),
            "debug output leaked raw destination: {debug}"
        );
    }
}
