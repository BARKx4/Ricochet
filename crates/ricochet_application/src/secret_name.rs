use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub struct SecretName(String);

pub struct SecretNameError;

impl SecretName {
    pub fn parse(value: &str) -> Result<Self, SecretNameError> {
        let bytes = value.as_bytes();
        if !(1..=128).contains(&bytes.len())
            || !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit()
            || !bytes.iter().skip(1).all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err(SecretNameError);
        }
        Ok(Self(value.to_string()))
    }
}

impl fmt::Debug for SecretName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secret-name>")
    }
}

impl fmt::Display for SecretName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secret-name>")
    }
}

impl fmt::Debug for SecretNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretNameError")
    }
}

impl fmt::Display for SecretNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid secret name")
    }
}

impl std::error::Error for SecretNameError {}

impl Serialize for SecretName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SecretName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}
