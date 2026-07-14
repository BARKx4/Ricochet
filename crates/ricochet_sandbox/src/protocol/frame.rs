use std::fmt;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroize;

use crate::error::{DiagnosticMetadata, SandboxError};
use crate::version::{FRAME_MAC_BYTES, MAX_FRAME_BYTES, PROTOCOL_V1};

use super::{EndpointRole, ProtocolEnvelope, ProtocolMessage};

type FrameMac = Hmac<Sha256>;

#[allow(clippy::result_large_err)]
fn protocol_error() -> SandboxError {
    SandboxError::protocol(DiagnosticMetadata::empty())
}

pub struct ProtocolKey([u8; 32]);

impl ProtocolKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for ProtocolKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProtocolKey([REDACTED])")
    }
}

impl Drop for ProtocolKey {
    fn drop(&mut self) {
        Zeroize::zeroize(&mut self.0);
    }
}

pub struct AuthenticatedCodec {
    local_role: EndpointRole,
    send_key: ProtocolKey,
    receive_key: ProtocolKey,
    next_send_sequence: u64,
    next_receive_sequence: u64,
}

impl AuthenticatedCodec {
    pub fn new(local_role: EndpointRole, send_key: ProtocolKey, receive_key: ProtocolKey) -> Self {
        Self {
            local_role,
            send_key,
            receive_key,
            next_send_sequence: 0,
            next_receive_sequence: 0,
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn encode(&mut self, message: ProtocolMessage) -> Result<Vec<u8>, SandboxError> {
        message.validate_for(self.local_role)?;
        let following_sequence = self
            .next_send_sequence
            .checked_add(1)
            .ok_or_else(protocol_error)?;
        let payload = serde_json::to_vec(&ProtocolEnvelope {
            protocol_version: PROTOCOL_V1,
            sequence: self.next_send_sequence,
            message,
        })
        .map_err(|_| protocol_error())?;
        if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
            return Err(protocol_error());
        }
        let payload_len = u32::try_from(payload.len()).map_err(|_| protocol_error())?;
        let mut frame = Vec::with_capacity(4 + payload.len() + FRAME_MAC_BYTES);
        frame.extend_from_slice(&payload_len.to_be_bytes());
        frame.extend_from_slice(&payload);
        let mut mac = FrameMac::new_from_slice(&self.send_key.0).map_err(|_| protocol_error())?;
        mac.update(&frame);
        frame.extend_from_slice(&mac.finalize().into_bytes());
        self.next_send_sequence = following_sequence;
        Ok(frame)
    }

    #[allow(clippy::result_large_err)]
    pub fn decode(&mut self, frame: &[u8]) -> Result<ProtocolMessage, SandboxError> {
        let length_prefix: [u8; 4] = frame
            .get(..4)
            .ok_or_else(protocol_error)?
            .try_into()
            .map_err(|_| protocol_error())?;
        let payload_len = u32::from_be_bytes(length_prefix) as usize;
        if payload_len == 0 || payload_len > MAX_FRAME_BYTES {
            return Err(protocol_error());
        }
        let payload_end = 4_usize
            .checked_add(payload_len)
            .ok_or_else(protocol_error)?;
        let expected_frame_len = payload_end
            .checked_add(FRAME_MAC_BYTES)
            .ok_or_else(protocol_error)?;
        if frame.len() != expected_frame_len {
            return Err(protocol_error());
        }

        let mut mac =
            FrameMac::new_from_slice(&self.receive_key.0).map_err(|_| protocol_error())?;
        mac.update(&frame[..payload_end]);
        mac.verify_slice(&frame[payload_end..])
            .map_err(|_| protocol_error())?;

        let envelope: ProtocolEnvelope =
            serde_json::from_slice(&frame[4..payload_end]).map_err(|_| protocol_error())?;
        if envelope.sequence != self.next_receive_sequence {
            return Err(protocol_error());
        }
        envelope.message.validate_for(self.local_role.peer())?;
        let following_sequence = self
            .next_receive_sequence
            .checked_add(1)
            .ok_or_else(protocol_error)?;
        self.next_receive_sequence = following_sequence;
        Ok(envelope.message)
    }
}

#[cfg(test)]
mod tests {
    use hmac::{Hmac, Mac};
    use serde_json::json;
    use sha2::Sha256;
    use zeroize::Zeroize;

    use super::*;
    use crate::{BrokerRequest, EndpointRole, ProtocolMessage, RequestId};

    fn signed_frame(key: &[u8; 32], payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(4 + payload.len() + 32);
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(payload);
        let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
        mac.update(&frame);
        frame.extend_from_slice(&mac.finalize().into_bytes());
        frame
    }

    #[test]
    fn protocol_key_can_be_explicitly_zeroized() {
        let mut key = ProtocolKey::from_bytes([0xa5; 32]);
        Zeroize::zeroize(&mut key.0);
        assert_eq!(key.0, [0; 32]);
    }

    #[test]
    fn send_sequence_exhaustion_is_a_non_consuming_protocol_error() {
        let mut codec = AuthenticatedCodec::new(
            EndpointRole::Host,
            ProtocolKey::from_bytes([7; 32]),
            ProtocolKey::from_bytes([9; 32]),
        );
        codec.next_send_sequence = u64::MAX;

        let error = codec
            .encode(ProtocolMessage::request(
                RequestId::new(1),
                BrokerRequest::Ping,
            ))
            .unwrap_err();

        assert_eq!(error.kind(), "BrokerProtocolError");
        assert_eq!(codec.next_send_sequence, u64::MAX);
    }

    #[test]
    fn receive_sequence_exhaustion_is_a_non_consuming_protocol_error() {
        let receive_key = [7; 32];
        let mut codec = AuthenticatedCodec::new(
            EndpointRole::Broker,
            ProtocolKey::from_bytes([9; 32]),
            ProtocolKey::from_bytes(receive_key),
        );
        codec.next_receive_sequence = u64::MAX;
        let payload = serde_json::to_vec(&json!({
            "protocol_version": 1,
            "sequence": u64::MAX,
            "message": {
                "type": "request",
                "body": {
                    "request_id": 1,
                    "request": { "type": "ping" }
                }
            }
        }))
        .unwrap();
        let frame = signed_frame(&receive_key, &payload);

        let error = codec.decode(&frame).unwrap_err();

        assert_eq!(error.kind(), "BrokerProtocolError");
        assert_eq!(codec.next_receive_sequence, u64::MAX);
    }
}
