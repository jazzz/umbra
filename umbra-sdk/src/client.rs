use crate::crypto;
use crate::error::UmbraError;

use std::sync::RwLock;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tracing::{debug, error};
use umbra_types::{
    EncryptedBytes, Message,
    payload::{
        ContentFrame, ToPayload,
        types::{Envelope, PayloadTags, TaggedPayload, frame},
    },
};
use umbra_types::{Frame, encrypted_bytes::*};

// Type Aliases for Identitifiers
pub type Addr = String;
// pub type AddrRef<'a> = &'a str;
pub type Blob = Vec<u8>;
// pub type ClientId = String;
// pub type ClientIdRef<'a> = &'a str;
// pub type ContentTopic = String;
// pub type ContentTopicRef<'a> = &'a str;

pub trait DeliveryService {
    fn send(&self, message: Blob) -> Result<(), UmbraError>;
    fn recv(&self) -> Result<Option<Blob>, UmbraError>;
}

/// Represents a conversation in the Umbra client.
pub struct Conversation<T: DeliveryService + Send + Sync + 'static> {
    convo_id: String,
    ds: Arc<Mutex<T>>,
}

impl<T> Conversation<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    pub fn new(convo_id: String, ds: Arc<Mutex<T>>) -> Self {
        Self { convo_id, ds }
    }

    // Returns an encoded payload for testing.
    pub fn send(&self, tag: u32, message: Blob) -> Vec<u8> {
        let frame = Frame {
            reliability_info: None,
            frame_type: Some(frame::FrameType::Content(ContentFrame {
                domain: 0,
                tag: tag,
                bytes: message,
            })),
        };

        let bytes = Envelope {
            encrypted_bytes: Some(Self::encrypt(frame)),
            conversation_id: self.convo_id.clone(),
        }
        .to_payload()
        .encode_to_vec();

        self.ds.lock().unwrap().send(bytes.clone()).unwrap();
        bytes
    }

    fn encrypt(frame: Frame) -> EncryptedBytes {
        EncryptedBytes {
            encryption: Some(Encryption::Reversed(Reversed {
                encrypted_bytes: crypto::encrypt_reverse(frame.encode_to_vec()),
            })),
        }
    }

    // returns any message which was not handled by this conversation
    pub fn recv(&self, enc_bytes: EncryptedBytes) -> Result<Option<Frame>, UmbraError> {
        let frame = Self::decrypt(enc_bytes)?;

        // Handle SDS data
        let _ = frame.reliability_info;

        match frame
            .frame_type
            .as_ref()
            .ok_or(UmbraError::DecodingError("bad packet".into()))?
        {
            frame::FrameType::Content(content_frame) => {
                debug!("conttent {:?}", content_frame);
                Ok(Some(frame))
            }
            frame::FrameType::ConversationInvite(conversation_invite) => {
                debug!("Invite {:?}", conversation_invite);
                Ok(Some(frame))
            }
        }
    }

    fn decrypt(enc_bytes: EncryptedBytes) -> Result<Frame, UmbraError> {
        // Check payload contained bytes
        let a = enc_bytes
            .encryption
            .ok_or(UmbraError::DecodingError("".into()))?;

        // Ensure the encryption type was "Reversed"
        let buf = if let Encryption::Reversed(r) = a {
            Ok(r.encrypted_bytes)
        } else {
            Err(UmbraError::DecodingError("Unsupported Enc".into()))
        }?;

        let plaintext = crypto::decrypt_reverse(buf);

        Frame::decode(plaintext.as_slice()).map_err(|e| UmbraError::DecodingError(e.to_string()))
    }
}
pub struct UmbraState<T: DeliveryService + Send + Sync + 'static> {
    convos: HashMap<Addr, Arc<Mutex<Conversation<T>>>>,
}

impl<T> UmbraState<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            convos: HashMap::new(),
        }
    }

    pub fn create_conversation(
        &mut self,
        ds: Arc<Mutex<T>>,
        addr: Addr,
    ) -> Option<Arc<Mutex<Conversation<T>>>> {
        let convo_id = addr.to_string();
        self.convos.insert(
            addr.clone(),
            Arc::new(Mutex::new(Conversation { convo_id, ds })),
        );

        self.get_conversation(addr)
    }

    fn get_conversation(&self, addr: Addr) -> Option<Arc<Mutex<Conversation<T>>>> {
        self.convos.get(&addr).cloned()
    }
}

pub struct UmbraClient<T: DeliveryService + Send + Sync + 'static> {
    ds: Arc<Mutex<T>>,
    state: Arc<RwLock<UmbraState<T>>>,
    on_content_handlers: Arc<RwLock<Vec<Box<dyn Fn(String, ContentFrame) + Send + Sync>>>>,
}

impl<T> UmbraClient<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    pub fn new(ds: T) -> Self {
        Self {
            ds: Arc::new(Mutex::new(ds)),
            state: Arc::new(RwLock::new(UmbraState::new())),
            on_content_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn start(&self) {
        let reciever = self.ds.clone();
        let state = self.state.clone();
        let handler = self.on_content_handlers.clone();
        std::thread::spawn(move || {
            loop {
                let incomming_bytes = reciever.lock().unwrap().recv().unwrap();

                if incomming_bytes.is_none() {
                    continue;
                }

                let incoming_bytes = incomming_bytes.unwrap();
                Self::recv(&state, &handler, incoming_bytes.as_slice())
                    .unwrap_or_else(|e| error!("Error receiving bytes: {:?}", e));
            }
        });
    }

    pub fn add_content_handler<F>(&mut self, handler: F)
    where
        F: Fn(String, ContentFrame) + Send + Sync + 'static,
    {
        self.on_content_handlers
            .write()
            .unwrap()
            .push(Box::new(handler));
    }

    pub fn address(&self) -> Addr {
        Addr::from("UmbraClient")
    }

    pub fn get_conversation(&self, addr: Addr) -> Option<Arc<Mutex<Conversation<T>>>> {
        let state = self.state.read().unwrap();
        state.get_conversation(addr)
    }

    pub fn create_conversation(&self, addr: Addr) -> Option<Arc<Mutex<Conversation<T>>>> {
        let mut state = self.state.write().unwrap();
        state.create_conversation(self.ds.clone(), addr)
    }

    pub fn recv(
        state: &Arc<RwLock<UmbraState<T>>>,
        handler: &Arc<RwLock<Vec<Box<dyn Fn(String, ContentFrame) + Send + Sync>>>>,
        bytes: &[u8],
    ) -> Result<(), UmbraError> {
        // Placeholder for receiving messages

        let payload =
            TaggedPayload::decode(bytes).map_err(|e| UmbraError::DecodingError(e.to_string()))?;

        match PayloadTags::from(payload.tag) {
            PayloadTags::Uknown => todo!(),
            PayloadTags::TagEnvelope => {
                let envelope = Envelope::decode(payload.payload_bytes.as_slice())
                    .map_err(|e| UmbraError::DecodingError(e.to_string()))?;
                Self::handle_envelope(&state, handler, envelope)
            }
            PayloadTags::TagPublicFrame => todo!(),
        }
    }

    fn handle_envelope(
        state: &Arc<RwLock<UmbraState<T>>>,
        handler: &Arc<RwLock<Vec<Box<dyn Fn(String, ContentFrame) + Send + Sync>>>>,
        payload: Envelope,
    ) -> Result<(), UmbraError> {
        debug!("ReceivedEnvelope: {:?}", payload);

        let enc = payload.encrypted_bytes.ok_or(UmbraError::DecodingError(
            "No encrypted bytes found".to_string(),
        ))?;

        let convo = state
            .read()
            .unwrap()
            .get_conversation(payload.conversation_id)
            .ok_or(UmbraError::DecodingError("No matching Conversation".into()))?
            .clone();

        let opt_frame = convo.lock().unwrap().recv(enc)?;

        // If no frame was returned, then all frames were parsed already - shortcircuit
        let frame = if let None = opt_frame {
            return Ok(());
        } else {
            opt_frame.unwrap()
        };

        match frame.frame_type.as_ref().unwrap() {
            frame::FrameType::Content(content_frame) => {
                for handler in handler.read().unwrap().iter() {
                    handler(
                        convo.lock().unwrap().convo_id.clone(),
                        content_frame.clone(),
                    );
                }

                Ok(())
            }
            frame::FrameType::ConversationInvite(conversation_invite) => {
                debug!("Received Conversation Invite: {:?}", conversation_invite);
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use prost::bytes::buf::Chain;

    use super::*;

    fn print_content(content_tag: u32, conversion_id: String, bytes: Vec<u8>) {
        print!(
            "Content received: tag={}, conversation_id={}, bytes={:?}\n",
            content_tag, conversion_id, bytes
        );
    }

    // #[test]
    // fn test_create_conversation() {
    //     let mut client = UmbraClient::new();
    //     client.add_content_handler(|String, content| match content.tag {
    //         0 => {
    //             println!("Unknown {:?}", content);
    //         }
    //         val if val == ContentTags::ContentTagChatMessage as u32 => {
    //             let msg = ChatMessage::decode(content.bytes.as_slice()).unwrap();

    //             println!("ChatMsg: {:?}", msg);
    //         }
    //         _ => {
    //             println!("Unknown Content Frame received with tag: {}", content.tag);
    //         }
    //     });
    //     let addr = Addr::from("test_addr");

    //     client.create_conversation(addr.clone());

    //     let convo = client.get_conversation(addr.clone()).unwrap();

    //     let cm = ChatMessage {
    //         text: "ABCDE".to_string(),
    //     }
    //     .encode_to_vec();

    //     let bytes = convo
    //         .lock()
    //         .unwrap()
    //         .send(ContentTags::ContentTagChatMessage as u32, cm);

    //     println!("Payload Sent: {:?}", bytes);

    //     client.recv(&bytes);
    // }
}
