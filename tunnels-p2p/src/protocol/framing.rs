//! Length-prefixed message framing codec.
//!
//! Messages are framed as:
//! - 4 bytes: network magic
//! - 4 bytes: big-endian message length
//! - N bytes: bincode-serialized Message

use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::config::{MAX_MESSAGE_SIZE, NETWORK_MAGIC};
use crate::error::{P2pError, P2pResult};
use crate::protocol::Message;

/// Header size: 4 bytes magic + 4 bytes length.
const HEADER_SIZE: usize = 8;

/// Codec for length-prefixed message framing.
#[derive(Debug, Default)]
pub struct MessageCodec {
    /// Expected length of the current message (if header has been read).
    current_length: Option<usize>,
}

impl MessageCodec {
    /// Create a new message codec.
    pub fn new() -> Self {
        Self {
            current_length: None,
        }
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = P2pError;

    fn decode(&mut self, src: &mut BytesMut) -> P2pResult<Option<Self::Item>> {
        // If we don't have the length yet, try to read the header
        if self.current_length.is_none() {
            if src.len() < HEADER_SIZE {
                // Not enough data for header
                return Ok(None);
            }

            // Read and verify magic bytes
            let magic: [u8; 4] = src[0..4].try_into().unwrap();
            if magic != NETWORK_MAGIC {
                return Err(P2pError::InvalidMagic {
                    expected: NETWORK_MAGIC,
                    actual: magic,
                });
            }

            // Read message length (big-endian)
            let length = u32::from_be_bytes(src[4..8].try_into().unwrap()) as usize;

            // Validate length
            if length > MAX_MESSAGE_SIZE {
                return Err(P2pError::MessageTooLarge {
                    size: length,
                    max: MAX_MESSAGE_SIZE,
                });
            }

            self.current_length = Some(length);
        }

        let length = self.current_length.unwrap();

        // Check if we have the full message
        if src.len() < HEADER_SIZE + length {
            // Reserve space for the full message to avoid reallocations
            src.reserve(HEADER_SIZE + length - src.len());
            return Ok(None);
        }

        // Skip header and extract message bytes
        src.advance(HEADER_SIZE);
        let message_bytes = src.split_to(length);

        // Reset state for next message
        self.current_length = None;

        // Deserialize the message
        let message: Message = tunnels_core::serialization::deserialize(&message_bytes)?;

        Ok(Some(message))
    }
}

impl Encoder<Message> for MessageCodec {
    type Error = P2pError;

    fn encode(&mut self, message: Message, dst: &mut BytesMut) -> P2pResult<()> {
        // Serialize the message
        let message_bytes = tunnels_core::serialization::serialize(&message)?;
        let length = message_bytes.len();

        // Validate length
        if length > MAX_MESSAGE_SIZE {
            return Err(P2pError::MessageTooLarge {
                size: length,
                max: MAX_MESSAGE_SIZE,
            });
        }

        // Reserve space
        dst.reserve(HEADER_SIZE + length);

        // Write magic bytes
        dst.put_slice(&NETWORK_MAGIC);

        // Write length (big-endian)
        dst.put_u32(length as u32);

        // Write message
        dst.put_slice(&message_bytes);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_ping() {
        let mut codec = MessageCodec::new();
        let original = Message::Ping(42);

        let mut buf = BytesMut::new();
        codec.encode(original.clone(), &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_roundtrip_version_ack() {
        let mut codec = MessageCodec::new();
        let original = Message::VersionAck;

        let mut buf = BytesMut::new();
        codec.encode(original.clone(), &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_partial_header() {
        let mut codec = MessageCodec::new();
        let mut buf = BytesMut::new();
        buf.put_slice(&NETWORK_MAGIC);
        // Only 4 bytes, not enough for header

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_partial_message() {
        let mut codec = MessageCodec::new();
        let mut buf = BytesMut::new();

        // Write valid header with large length
        buf.put_slice(&NETWORK_MAGIC);
        buf.put_u32(100); // 100 bytes expected
        buf.put_slice(&[0u8; 50]); // Only 50 bytes

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_magic() {
        let mut codec = MessageCodec::new();
        let mut buf = BytesMut::new();

        buf.put_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // Wrong magic
        buf.put_u32(10);
        buf.put_slice(&[0u8; 10]);

        let result = codec.decode(&mut buf);
        assert!(matches!(result, Err(P2pError::InvalidMagic { .. })));
    }

    #[test]
    fn test_message_too_large() {
        let mut codec = MessageCodec::new();
        let mut buf = BytesMut::new();

        buf.put_slice(&NETWORK_MAGIC);
        buf.put_u32((MAX_MESSAGE_SIZE + 1) as u32);

        let result = codec.decode(&mut buf);
        assert!(matches!(result, Err(P2pError::MessageTooLarge { .. })));
    }

    #[test]
    fn test_multiple_messages() {
        let mut codec = MessageCodec::new();
        let mut buf = BytesMut::new();

        // Encode two messages
        codec.encode(Message::Ping(1), &mut buf).unwrap();
        codec.encode(Message::Pong(2), &mut buf).unwrap();

        // Decode first
        let msg1 = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(msg1, Message::Ping(1));

        // Decode second
        let msg2 = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(msg2, Message::Pong(2));

        // No more messages
        assert!(buf.is_empty());
    }
}
