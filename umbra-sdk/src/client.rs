use prost::Message;
use std::sync::RwLock;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tracing::{Level, debug, error, span, warn};
use umbra_types::base::{
    EncryptedBytes, InboxV1Frame, UmbraEnvelopeV1, encrypted_bytes, inbox_v1_frame,
};
use umbra_types::common_frames::ContentFrame;
use umbra_types::convos::private_v1::{self};
use umbra_types::encryption;
use umbra_types::invite;
use umbra_types::payload::ToEnvelope;

use crate::convos::private::PrivateConversation;
use crate::error::UmbraError;

// Type Aliases for Identitifiers
pub type Addr = String;
pub type Blob = Vec<u8>;

pub trait DeliveryService {
    fn send(&self, message: Blob) -> Result<(), UmbraError>;
    fn recv(&self) -> Result<Option<Blob>, UmbraError>;
}

pub trait Conversation<T: DeliveryService + Send + Sync + 'static> {
    fn convo_id(&self) -> String;
    fn send(&self, tag: u32, message: Blob) -> Vec<u8>;
    fn recv(&self, enc_bytes: EncryptedBytes) -> Result<(), UmbraError>;
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
        let convo_id = topic_private_convo(addrs); //TODO: conversations need to determine their ContentTopic

        debug!("Register convo: {}", convo_id);
        self.convos.insert(
            convo_id.clone(),
            Arc::new(Mutex::new(PrivateConversation::new(convo_id.clone(), ds))),
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
    inbox_topic: String,
    ds: Arc<Mutex<T>>,
    state: Arc<RwLock<UmbraState<T>>>,
    on_content_handlers: Arc<RwLock<Vec<Box<dyn Fn(String, ContentFrame) + Send + Sync>>>>,
}

impl<T> UmbraClient<T>
where
    T: DeliveryService + Send + Sync + 'static,
{
    pub fn new(ds: T, addr: Addr) -> Self {
        let inbox_topic = topic_inbox_convo(&addr);

        Self {
            addr,
            inbox_topic,
            ds: Arc::new(Mutex::new(ds)),
            state: Arc::new(RwLock::new(UmbraState::new())),
            on_content_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn start(&mut self) {
        {
            let x = self.state.write().unwrap();
        }

        let self_topic = self.inbox_topic.clone();
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
                Self::recv(
                    &state,
                    &ds,
                    &handler,
                    &self_topic,
                    incoming_bytes.as_slice(),
                )
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

    pub fn create_private_conversation(
        &self,
        addr: Addr,
    ) -> Result<Arc<Mutex<dyn Conversation<T> + Send + Sync + 'static>>, UmbraError> {
        let topic = format!("/inbox/{}", addr);

        let addrs = vec![self.address(), addr.clone()];

        // Create Local side
        let mut state = self.state.write().unwrap();
        let convo = state.create_conversation(self.ds.clone(), addrs.clone());
        let convo = convo.ok_or_else(|| UmbraError::UnexpectedError)?;

        self.send_invite(addr)?;

        Ok(convo)
    }

    fn send_invite(&self, recipient: String) -> Result<(), UmbraError> {
        let invite = inbox_v1_frame::FrameType::InvitePrivateV1(invite::InvitePrivateV1 {
            participants: sorted_pariticipants(vec![self.address(), recipient.clone()]),
        });

        let frame = InboxV1Frame::new("conversationID".into(), invite);

        let encrypted_bytes = EncryptedBytes {
            encryption: Some(encrypted_bytes::Encryption::Plaintext(
                encryption::Plaintext {
                    payload: frame.encode_to_vec(),
                },
            )),
        };

        self.ds.lock().unwrap().send(
            encrypted_bytes
                .to_envelope(topic_inbox_convo(&recipient), 0)
                .encode_to_vec(),
        )
    }

    pub fn recv(
        state: &Arc<RwLock<UmbraState<T>>>,
        ds: &Arc<Mutex<T>>,
        handler: &Arc<RwLock<Vec<Box<dyn Fn(String, ContentFrame) + Send + Sync>>>>,
        topic: &str,
        bytes: &[u8],
    ) -> Result<(), UmbraError> {
        // Placeholder for receiving messages

        let envelope = UmbraEnvelopeV1::decode(bytes)
            .map_err(|e| UmbraError::DecodingError(e.to_string()))
            .expect(format!("Failed to decode UmbraEnvelopeV1: {:?}", bytes).as_str());

        Self::handle_envelope(state, ds, handler, envelope, topic)
    }

    fn get_conversation_by_hint(
        state: &Arc<RwLock<UmbraState<T>>>,
        hint: String,
        salt: u64,
    ) -> Option<Arc<Mutex<dyn Conversation<T> + Send + Sync>>> {
        state.read().unwrap().get_conversation(hint)
    }

    // In the future the payload type will be tightly coupled to the Conversation
    fn handle_envelope(
        state: &Arc<RwLock<UmbraState<T>>>,
        ds: &Arc<Mutex<T>>,
        handler: &Arc<RwLock<Vec<Box<dyn Fn(String, ContentFrame) + Send + Sync>>>>,
        payload: UmbraEnvelopeV1,
        self_topic: &str,
    ) -> Result<(), UmbraError> {
        debug!("ReceivedEnvelope: {:?}", payload);

        if payload.conversation_hint == self_topic {
            debug!("Received Inbox Envelope: {:?}", payload);
            let enc_bytes = EncryptedBytes::decode(&*payload.payload)?;

            Self::handle_invite(state, ds, enc_bytes)?;
        }

        let res_convo =
            Self::get_conversation_by_hint(state, payload.conversation_hint.clone(), payload.salt);

        // TODO: Don't ignore missing conversations
        if let None = res_convo {
            debug!("No matching Conversation ({})", payload.conversation_hint);
            return Ok(());
        }
        let enc = EncryptedBytes::decode(&*payload.payload)?;
        let convo = res_convo.unwrap().clone();

        convo.lock().unwrap().recv(enc)
    }

    fn handle_invite(
        state: &Arc<RwLock<UmbraState<T>>>,
        ds: &Arc<Mutex<T>>,
        encrypted_invite: EncryptedBytes,
    ) -> Result<(), UmbraError> {
        if !matches!(
            encrypted_invite.encryption,
            Some(encrypted_bytes::Encryption::Plaintext(_))
        ) {
            warn!("Invalid Encryption Type for Invite");
        }

        let bytes = if let encrypted_bytes::Encryption::Plaintext(b) =
            encrypted_invite.encryption.unwrap()
        {
            b.payload
        } else {
            return Err(UmbraError::DecodingError(
                "Invalid Encryption Type for Invite".into(),
            ));
        };

        let convo_frame = InboxV1Frame::decode(bytes.as_slice())
            .map_err(|e| UmbraError::DecodingError(e.to_string()))?;

        match convo_frame
            .frame_type
            .as_ref()
            .ok_or(UmbraError::DecodingError("bad packet".into()))?
        {
            inbox_v1_frame::FrameType::InvitePrivateV1(invite) => {
                state
                    .write()
                    .unwrap()
                    .create_conversation(ds.clone(), invite.participants.clone())
                    .ok_or_else(|| UmbraError::UnexpectedError)?;
            }
        };

        Ok(())
    }
}

fn topic_private_convo(mut addrs: Vec<String>) -> String {
    addrs.sort();
    let topic = addrs.join("|");
    format!("/private/{}", topic)
}

fn topic_inbox_convo(addr: &str) -> String {
    format!("/inbox/{}", addr)
}

fn sorted_pariticipants(mut participants: Vec<String>) -> Vec<String> {
    participants.sort();
    participants
}
