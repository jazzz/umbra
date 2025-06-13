use crate::crypto;
use crate::error::UmbraError;

use std::sync::RwLock;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tracing::{Level, debug, error, info, span, warn};
use umbra_types::payload::ConversationInvite;
use umbra_types::{
    EncryptedBytes, Message,
    payload::{
        ContentFrame, ToPayload,
        types::{Envelope, PayloadTags, TaggedPayload, frame},
    },
};
use umbra_types::{Frame, ToFrame, encrypted_bytes::*};

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

pub trait Conversation<T: DeliveryService + Send + Sync + 'static> {
    fn convo_id(&self) -> String;
    fn send(&self, tag: u32, message: Blob) -> Vec<u8>;
    fn recv(&self, enc_bytes: EncryptedBytes) -> Result<Option<Frame>, UmbraError>;
}

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

    fn encrypt(frame: Frame) -> EncryptedBytes {
        EncryptedBytes {
            encryption: Some(Encryption::Reversed(Reversed {
                encrypted_bytes: crypto::encrypt_reverse(frame.encode_to_vec()),
            })),
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

impl<T> Conversation<T> for PrivateConversation<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    // Returns an encoded payload for testing.
    fn send(&self, tag: u32, message: Blob) -> Vec<u8> {
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

    // returns any message which was not handled by this conversation
    fn recv(&self, enc_bytes: EncryptedBytes) -> Result<Option<Frame>, UmbraError> {
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

    fn convo_id(&self) -> String {
        self.convo_id.clone()
    }
}

pub struct InboxConversation<T: DeliveryService + Send + Sync + 'static> {
    convo_id: String,
    _ds: Arc<Mutex<T>>,
}

impl<T> InboxConversation<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    pub fn new(convo_id: String, ds: Arc<Mutex<T>>) -> Self {
        Self { convo_id, _ds: ds }
    }

    #[allow(dead_code)]
    fn encrypt(frame: Frame) -> EncryptedBytes {
        EncryptedBytes {
            encryption: Some(Encryption::Plaintext(Plaintext {
                bytes: frame.encode_to_vec(),
            })),
        }
    }

    fn decrypt(enc_bytes: EncryptedBytes) -> Result<Frame, UmbraError> {
        // Check payload contained bytes
        let a = enc_bytes
            .encryption
            .ok_or(UmbraError::DecodingError("".into()))?;

        // Ensure the encryption type was "Reversed"
        let buf = if let Encryption::Plaintext(r) = a {
            Ok(r.bytes)
        } else {
            Err(UmbraError::DecodingError("Unsupported Enc".into()))
        }?;

        Frame::decode(buf.as_slice()).map_err(|e| UmbraError::DecodingError(e.to_string()))
    }
}

impl<T> Conversation<T> for InboxConversation<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    // Returns an encoded payload for testing.
    fn send(&self, _tag: u32, _message: Blob) -> Vec<u8> {
        panic!("This should never be called. Clients would never send messages to themselves")
    }

    // returns any message which was not handled by this conversation
    fn recv(&self, enc_bytes: EncryptedBytes) -> Result<Option<Frame>, UmbraError> {
        let frame = Self::decrypt(enc_bytes)?;

        // Handle SDS data
        let _ = frame.reliability_info;

        match frame
            .frame_type
            .as_ref()
            .ok_or(UmbraError::DecodingError("bad packet".into()))?
        {
            frame::FrameType::Content(content_frame) => {
                warn!(
                    "Content recieved on InboxTopic. Dropping. {:?}",
                    content_frame
                );
                Ok(None)
            }
            frame::FrameType::ConversationInvite(conversation_invite) => {
                debug!("Invite {:?}", conversation_invite);
                Ok(Some(frame))
            }
        }
    }

    fn convo_id(&self) -> String {
        self.convo_id.clone()
    }
}

pub struct UmbraState<T: DeliveryService + Send + Sync + 'static> {
    convos: HashMap<Addr, Arc<Mutex<dyn Conversation<T> + Send + Sync>>>,
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
        addrs: Vec<Addr>,
    ) -> Option<Arc<Mutex<dyn Conversation<T> + Send + Sync>>> {
        let convo_id = topic_private_convo(addrs);
        self.convos.insert(
            convo_id.clone(),
            Arc::new(Mutex::new(PrivateConversation::new(convo_id.clone(), ds))),
        );

        self.get_conversation(convo_id)
    }

    pub fn create_inbox_convo(
        &mut self,
        ds: Arc<Mutex<T>>,
        addr: Addr,
    ) -> Option<Arc<Mutex<dyn Conversation<T> + Send + Sync>>> {
        let convo_id = topic_inbox_convo(addr);

        info!("Register convo: {}", convo_id);
        self.convos.insert(
            convo_id.clone(),
            Arc::new(Mutex::new(InboxConversation::new(convo_id.clone(), ds))),
        );

        self.get_conversation(convo_id)
    }

    fn get_conversation(
        &self,
        addr: Addr,
    ) -> Option<Arc<Mutex<dyn Conversation<T> + Send + Sync>>> {
        self.convos.get(&addr).cloned()
    }
}

pub struct UmbraClient<T: DeliveryService + Send + Sync + 'static> {
    addr: Addr,
    ds: Arc<Mutex<T>>,
    state: Arc<RwLock<UmbraState<T>>>,
    on_content_handlers: Arc<RwLock<Vec<Box<dyn Fn(String, ContentFrame) + Send + Sync>>>>,
}

impl<T> UmbraClient<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    pub fn new(ds: T, addr: Addr) -> Self {
        Self {
            addr,
            ds: Arc::new(Mutex::new(ds)),
            state: Arc::new(RwLock::new(UmbraState::new())),
            on_content_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn start(&self) {
        {
            self.state
                .write()
                .unwrap()
                .create_inbox_convo(self.ds.clone(), self.address());
        }

        let ds = self.ds.clone();
        let state = self.state.clone();
        let handler = self.on_content_handlers.clone();
        let addr = self.address();
        std::thread::spawn(move || {
            let span = span!(Level::INFO, "RecvThread", addr = addr);
            let _enter = span.enter();
            loop {
                let incomming_bytes = ds.lock().unwrap().recv().unwrap();

                if incomming_bytes.is_none() {
                    continue;
                }

                let incoming_bytes = incomming_bytes.unwrap();
                Self::recv(&state, &ds, &handler, incoming_bytes.as_slice())
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
        self.addr.clone()
    }

    pub fn get_conversation(
        &self,
        addr: Addr,
    ) -> Option<Arc<Mutex<dyn Conversation<T> + Send + Sync>>> {
        let state = self.state.read().unwrap();
        state.get_conversation(addr)
    }

    pub fn create_conversation(
        &self,
        addr: Addr,
    ) -> Result<Arc<Mutex<dyn Conversation<T> + Send + Sync + 'static>>, UmbraError> {
        let topic = format!("/inbox/{}", addr);

        let addrs = vec![self.address(), addr.clone()];

        let mut state = self.state.write().unwrap();
        let convo = state.create_conversation(self.ds.clone(), addrs.clone());

        let convo = convo.ok_or_else(|| UmbraError::UnexpectedError)?;

        let msg = ConversationInvite::new(addrs).to_frame(None);
        self.send_frame(topic, msg)?;

        Ok(convo)
    }

    pub fn send_frame(&self, convo_id: String, frame: Frame) -> Result<(), UmbraError> {
        let encrypted = EncryptedBytes {
            encryption: Some(Encryption::Plaintext(Plaintext {
                bytes: frame.encode_to_vec(),
            })),
        };

        let bytes = Envelope {
            encrypted_bytes: Some(encrypted),
            conversation_id: convo_id,
        }
        .to_payload()
        .encode_to_vec();

        self.ds.lock().unwrap().send(bytes.clone())
    }

    pub fn recv(
        state: &Arc<RwLock<UmbraState<T>>>,
        ds: &Arc<Mutex<T>>,
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
                Self::handle_envelope(&state, ds, handler, envelope)
            }
            PayloadTags::TagPublicFrame => todo!(),
        }
    }

    // In the future the payload type will be tightly coupled to the Conversation
    fn handle_envelope(
        state: &Arc<RwLock<UmbraState<T>>>,
        ds: &Arc<Mutex<T>>,
        handler: &Arc<RwLock<Vec<Box<dyn Fn(String, ContentFrame) + Send + Sync>>>>,
        payload: Envelope,
    ) -> Result<(), UmbraError> {
        debug!("ReceivedEnvelope: {:?}", payload);

        let enc = payload.encrypted_bytes.ok_or(UmbraError::DecodingError(
            "No encrypted bytes found".to_string(),
        ))?;

        let res_convo = state
            .read()
            .unwrap()
            .get_conversation(payload.conversation_id.clone());

        // TODO: Don't ignore missing conversations
        if let None = res_convo {
            debug!("No matching Conversation ({})", payload.conversation_id);
            return Ok(());
        }
        let convo = res_convo.unwrap().clone();

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
                        convo.lock().unwrap().convo_id().clone(),
                        content_frame.clone(),
                    );
                }

                Ok(())
            }
            frame::FrameType::ConversationInvite(conversation_invite) => {
                info!("Received Conversation Invite: {:?}", conversation_invite);

                state
                    .write()
                    .unwrap()
                    .create_conversation(ds.clone(), conversation_invite.participants.clone());
                return Ok(());
            }
        }
    }
}

fn topic_private_convo(mut addrs: Vec<String>) -> String {
    addrs.sort();
    let topic = addrs.join("|");
    format!("/private/{}", topic)
}

fn topic_inbox_convo(addr: String) -> String {
    format!("/inbox/{}", addr)
}
