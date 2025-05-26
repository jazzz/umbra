use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};

use crate::crypto::{self, encrypt_reverse};
use crate::utils;
use thiserror::Error;
use tracing::{debug, info};
use umbra_types::payload::ToPayload;
use umbra_types::payload::types::TaggedPayload;
use umbra_types::{ChatMessage, Message, ToFrame, encrypted_bytes::Reversed};
use umbra_types::{EncryptedBytes, Frame};

#[derive(Debug, Error)]
pub enum UmbraError {
    #[error("Failed to publish message to topic: {0}")]
    PublishError(String),

    #[error("Failed to poll messages for client: {0}")]
    PollError(String),

    #[error("Problem encoding type: {0}")]
    EncodingError(String),

    #[error("Problem decoding type: {0}")]
    DecodingError(String),

    #[error("Unknown error occurred")]
    UnexpectedError,

    #[error("Unknown error occurred")]
    TodoError,
}

enum ConversationType {
    Private,
    Group,
    Forum,
    Broadcast,
    GatedGroup,
}

pub type Addr = String;
pub type AddrRef<'a> = &'a str;
pub type Blob = Vec<u8>;
pub type ClientId = String;
pub type ClientIdRef<'a> = &'a str;
pub type ContentTopic = String;
pub type ContentTopicRef<'a> = &'a str;

type Message2 = ChatMessage;

pub trait Publish {
    fn publish(&self, topic: ContentTopic, value: TaggedPayload);
}

//To be replaced by Subscribe
pub trait Poll {
    fn poll(&self, client_id: ClientIdRef, topic: &str) -> Vec<Vec<u8>>;
}

pub trait DS: Publish + Poll {}

pub trait Query {}
pub trait Store {}

pub struct InMemMsgStore {}

impl Query for InMemMsgStore {}
impl Store for InMemMsgStore {}

#[derive(Clone)]
pub struct Identity {
    id: String,
}
impl Identity {
    pub fn new_ephemeral() -> Self {
        Self {
            id: utils::generate_random_string(16),
        }
    }

    pub fn address(&self) -> &str {
        &self.id
    }
}

pub struct ConversationSession<'a> {
    ds: &'a dyn DS,
    id: String,
    owner: Identity,
    participants: Identity,
}

impl<'a> ConversationSession<'a> {
    fn new(ds: &'a dyn DS, id: String, owner: Identity, participants: Identity) -> Self {
        Self {
            ds,
            id,
            owner,
            participants,
        }
    }

    fn encrypt(&self, msg: Message2) -> umbra_types::EncryptedBytes {
        let buf = msg.to_frame(None).encode_to_vec();
        let encrypted_bytes = encrypt_reverse(buf);
        umbra_types::EncryptedBytes {
            algo: Some(umbra_types::encrypted_bytes::Algo::Reversed(Reversed {
                encrypted_bytes,
            })),
        }
    }

    pub fn send(&self, msg: Message2) -> Result<(), UmbraError> {
        self.ds.publish(
            topic_inbox(self.participants.address()),
            self.encrypt(msg).to_payload(),
        );
        Ok(())
    }
}

pub struct ConversationGroup {}

pub struct UmbraClient<'a> {
    ident: Identity,
    ds: &'a dyn DS,
    store: &'a dyn Store,

    handlers_on_conversation: Vec<Box<dyn Fn(ConversationSession) + Send + Sync>>,
    handlers_on_conversation_update: Vec<Box<dyn Fn(String) + Send + Sync>>,
    handlers_on_mesage: Vec<Box<dyn Fn(ChatMessage) + Send + Sync>>,

    // Testing Vars
    known_contacts: HashMap<Addr, Identity>,
    sessions: HashMap<Addr, RefCell<ConversationSession<'a>>>,
    subscriptions: HashSet<ContentTopic>,
}

impl<'a> UmbraClient<'a> {
    fn new(ident: Identity, ds: &'a dyn DS, store: &'a dyn Store) -> Self {
        info!(ident = ident.address(), "Client created");

        Self {
            ident,
            ds,
            store,
            handlers_on_conversation: vec![],
            handlers_on_conversation_update: vec![],
            handlers_on_mesage: vec![],

            known_contacts: HashMap::new(),
            sessions: HashMap::new(),
            subscriptions: HashSet::new(),
        }
    }

    pub fn create_with_ephemeral_identity(
        ds: &'a dyn DS,
        store: &'a dyn Store,
    ) -> Result<Self, UmbraError> {
        let identity = Identity::new_ephemeral();
        Ok(Self::new(identity, ds, store))
    }

    pub fn address(&self) -> &str {
        self.ident.address()
    }

    pub fn create_session(
        &'a mut self,
        participant_addr: AddrRef,
    ) -> Result<&RefCell<ConversationSession>, UmbraError> {
        let convo_id = utils::generate_random_string(16);
        let owner = self.ident.clone();
        let participant = self
            .lookup_identity(participant_addr)
            .ok_or_else(|| UmbraError::UnexpectedError)?
            .clone();

        self.subscribe_to_topic(topic_inbox(participant.address()));

        // This is ugly
        let k = participant.clone();

        let sesh = ConversationSession::new(self.ds, convo_id.clone(), owner, participant);

        self.save_session(sesh)?;

        self.sessions
            .get(k.address())
            .ok_or_else(|| UmbraError::UnexpectedError)
    }

    pub fn save_session(&mut self, session: ConversationSession<'a>) -> Result<(), UmbraError> {
        let addr = session.participants.address().to_string();
        self.sessions.insert(addr, RefCell::new(session));
        Ok(())
    }

    pub fn create_group(
        &self,
        _participants: &[Identity],
    ) -> Result<ConversationGroup, UmbraError> {
        todo!()
    }

    pub fn send_message(
        &self,
        convo: &ConversationSession,
        msg: ChatMessage,
    ) -> Result<(), UmbraError> {
        let topic = format!("inbox/{}", convo.id);
        self.ds.publish(topic, convo.encrypt(msg).to_payload());
        Ok(())
    }

    // This function is to be removed.
    pub fn poll(&self) -> Result<(), UmbraError> {
        for subs in self.subscriptions.iter() {
            let msg_bufs = self.ds.poll(self.address(), subs);

            for buf in msg_bufs {
                self.handle_incoming_message(buf);
            }
        }

        Ok(())
    }

    fn handle_incoming_message(&self, buf: Vec<u8>) -> Result<(), UmbraError> {
        let tagged_payload = TaggedPayload::decode(buf.as_slice()).unwrap();
        let tag = tagged_payload.tag;

        let frame = match tag {
            1 => {
                let encrypted_bytes =
                    EncryptedBytes::decode(tagged_payload.payload_bytes.as_slice()).unwrap();

                let frame = self.decrypt(encrypted_bytes);
                debug!("Decrypted frame: {:?}", frame);
                Some(frame)
            }
            _ => {
                info!("Unknown tag: {}", tag);
                None
            }
        }
        .ok_or_else(|| UmbraError::DecodingError("Failed to decode frame".to_string()))?;

        //TODO: Process frame
    }

    fn encrypt(&self, frame: Frame) -> EncryptedBytes {
        let bytes = crypto::encrypt_reverse(frame.encode_to_vec());
        EncryptedBytes {
            algo: Some(umbra_types::encrypted_bytes::Algo::Reversed(Reversed {
                encrypted_bytes: bytes,
            })),
        }
    }

    fn decrypt(&self, enc_bytes: EncryptedBytes) -> Frame {
        match enc_bytes.algo {
            Some(umbra_types::encrypted_bytes::Algo::Reversed(rev)) => {
                let bytes = rev.encrypted_bytes;
                let decrypted_bytes = crypto::decrypt_reverse(bytes);
                Frame::decode(decrypted_bytes.as_slice()).unwrap()
            }
            _ => panic!("Unsupported encryption algorithm"),
        }
    }

    pub fn get_conversation(&'a self, addr: &str) -> Option<&RefCell<ConversationSession<'a>>> {
        self.sessions.get(addr)
    }

    pub fn list_conversations(&'a self) -> impl Iterator<Item = &RefCell<ConversationSession<'a>>> {
        return self.sessions.values().into_iter();
    }

    pub fn on_new_conversation<F>(&mut self, callback: F)
    where
        F: Fn(ConversationSession) + Send + Sync + 'static,
    {
        self.handlers_on_conversation.push(Box::new(callback));
    }

    pub fn on_conversation_update<F>(&mut self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.handlers_on_conversation_update
            .push(Box::new(callback));
    }

    // Assume Ident = Addr for now
    fn lookup_identity(&mut self, addr: AddrRef) -> Option<&Identity> {
        // Skip registration lookup

        let ident = Identity {
            id: addr.to_string(),
        };

        self.known_contacts.insert(addr.to_string(), ident);
        self.known_contacts.get(addr)
    }

    fn subscribe_to_topic(&mut self, topic: ContentTopic) {
        self.subscriptions.insert(topic);
    }
}

fn gen_conversation_id(participants: &[AddrRef]) -> String {
    let s = participants.join("|");
    crypto::hash_string(&s)
}

fn topic_inbox(participant: AddrRef) -> String {
    format!("inbox/{}", participant)
}

/*
    // Sensible default constructors
    static Client createClientWithNewAccount(....)
    static Client createClientWithExistingAccount(....);

}
    */

// struct Conversation {
// 		ConversationType conversationType;

// 		bool addParticipant();
// 		bool removeParticipant();
// 	  ...
// );
