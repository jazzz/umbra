use std::sync::{Arc, Mutex};

use prost::Message;
use tracing::{debug, info};
use umbra_types::{
    base::{EncryptedBytes, ReliableBytes, encrypted_bytes},
    common_frames::ContentFrame,
    convos::private_v1::{PrivateV1Frame, private_v1_frame},
    encryption,
    payload::ToEnvelope,
};

use crate::{Blob, Conversation, DeliveryService, UmbraError, crypto};

/// Represents a conversation in the Umbra client.
pub struct PrivateConversation<T: DeliveryService + Send + Sync + 'static> {
    convo_id: String,
    ds: Arc<Mutex<T>>,
}

impl<T> PrivateConversation<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    pub fn new(convo_id: String, ds: Arc<Mutex<T>>) -> Self {
        Self { convo_id, ds }
    }

    fn encrypt(&self, frame: &[u8]) -> EncryptedBytes {
        EncryptedBytes {
            encryption: Some(encrypted_bytes::Encryption::Plaintext(
                encryption::Plaintext {
                    payload: frame.to_vec(),
                },
            )),
        }
    }

    fn decrypt(enc_bytes: EncryptedBytes) -> Result<ReliableBytes, UmbraError> {
        // Ensure the encryption type was "???"
        let buf = if let encrypted_bytes::Encryption::Plaintext(r) = enc_bytes.encryption.unwrap() {
            Ok(r.payload)
        } else {
            Err(UmbraError::DecodingError("Unsupported Enc".into()))
        }?;

        let plaintext = buf;

        ReliableBytes::decode(plaintext.as_slice())
            .map_err(|e| UmbraError::DecodingError(e.to_string()))
    }
}

impl<T> Conversation<T> for PrivateConversation<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    // Returns an encoded payload for testing.
    fn send(&self, tag: u32, message: Blob) -> Vec<u8> {
        // Build Frame
        let frame = PrivateV1Frame {
            conversation_id: self.convo_id(),
            frame_type: Some(private_v1_frame::FrameType::Content(ContentFrame {
                domain: 0,
                tag,
                bytes: message,
            })),
        };

        let encoded_frame = frame.encode_to_vec();

        // Wrap in Reliable Bytes
        let reliable_bytes = ReliableBytes {
            message_id: crypto::hash_to_string(&encoded_frame),
            channel_id: self.convo_id(),
            lamport_timestamp: 0,
            causal_history: vec![],
            bloom_filter: vec![],
            content: Some(encoded_frame),
        };

        // Encrypt and Wrap in Envelope
        let bytes = self
            .encrypt(&reliable_bytes.encode_to_vec())
            .to_envelope(self.convo_id(), 0)
            .encode_to_vec();

        self.ds.lock().unwrap().send(bytes.clone()).unwrap();
        bytes
    }

    // returns any message which was not handled by this conversation
    fn recv(&self, enc_bytes: EncryptedBytes) -> Result<(), UmbraError> {
        let sds_frame = Self::decrypt(enc_bytes)?;

        info!("Received SDS Frame: {:?}", sds_frame);
        // Handle SDS data
        let convo_frame = PrivateV1Frame::decode(sds_frame.content())?;

        match convo_frame
            .frame_type
            .as_ref()
            .ok_or(UmbraError::DecodingError("bad packet".into()))?
        {
            private_v1_frame::FrameType::Content(frame) => {
                info!("conttent {:?}", frame);
            }
            private_v1_frame::FrameType::Placeholder(frame) => {
                info!("placeholder {:?}", frame);
            }
        };

        Ok(())
    }

    fn convo_id(&self) -> String {
        self.convo_id.clone()
    }
}
