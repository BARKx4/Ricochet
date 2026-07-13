use std::fmt;

use serde::de::Error as _;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::error::{DiagnosticMetadata, FailedGuarantee, SandboxError};

fn validation_error() -> SandboxError {
    SandboxError::policy(FailedGuarantee::PolicyValidity, DiagnosticMetadata::empty())
}

fn valid_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        })
}

macro_rules! validated_string_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            #[allow(clippy::result_large_err)]
            pub fn parse(value: impl Into<String>) -> Result<Self, SandboxError> {
                let value = value.into();
                if valid_identifier(&value) {
                    Ok(Self(value))
                } else {
                    Err(validation_error())
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(value).map_err(D::Error::custom)
            }
        }
    };
}

validated_string_id!(SessionId);
validated_string_id!(ScratchId);
validated_string_id!(ToolId);
validated_string_id!(BackendFeatureId);

macro_rules! numeric_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            pub fn new(value: u64) -> Self {
                Self(value)
            }

            pub fn get(self) -> u64 {
                self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_u64(self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Ok(Self::new(u64::deserialize(deserializer)?))
            }
        }
    };
}

numeric_id!(RequestId);
numeric_id!(ProcessId);
numeric_id!(PtyId);
numeric_id!(ProcessTreeId);
numeric_id!(UnixMillis);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogGeneration(u64);

impl CatalogGeneration {
    #[allow(clippy::result_large_err)]
    pub fn new(value: u64) -> Result<Self, SandboxError> {
        if value == 0 {
            Err(validation_error())
        } else {
            Ok(Self(value))
        }
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl Serialize for CatalogGeneration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for CatalogGeneration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u64::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[allow(clippy::result_large_err)]
    pub fn parse_hex(value: &str) -> Result<Self, SandboxError> {
        if value.len() != 64 {
            return Err(validation_error());
        }

        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_lower_hex(pair[0]).ok_or_else(validation_error)?;
            let low = decode_lower_hex(pair[1]).ok_or_else(validation_error)?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }

    pub fn to_hex(&self) -> String {
        const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(64);
        for byte in self.0 {
            encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
        }
        encoded
    }

    pub fn hash(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut result = [0_u8; 32];
        result.copy_from_slice(&digest);
        Self(result)
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Sha256Digest")
            .field(&self.to_hex())
            .finish()
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_hex(&value).map_err(D::Error::custom)
    }
}

fn decode_lower_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyDigest(Sha256Digest);

impl PolicyDigest {
    pub fn from_sha256(value: Sha256Digest) -> Self {
        Self(value)
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

impl fmt::Debug for PolicyDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("PolicyDigest")
            .field(&self.to_hex())
            .finish()
    }
}

impl Serialize for PolicyDigest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for PolicyDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(Sha256Digest::deserialize(deserializer)?))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackendIdentity {
    name: String,
    version: String,
}

impl BackendIdentity {
    #[allow(clippy::result_large_err)]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Result<Self, SandboxError> {
        let name = name.into();
        let version = version.into();
        if !valid_identifier(&name)
            || !(1..=64).contains(&version.len())
            || !version.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(validation_error());
        }
        Ok(Self { name, version })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

impl Serialize for BackendIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("BackendIdentity", 2)?;
        state.serialize_field("name", self.name())?;
        state.serialize_field("version", self.version())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for BackendIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawBackendIdentity {
            name: String,
            version: String,
        }

        let raw = RawBackendIdentity::deserialize(deserializer)?;
        Self::new(raw.name, raw.version).map_err(D::Error::custom)
    }
}
