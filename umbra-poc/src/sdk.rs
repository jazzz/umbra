use crate::crypto::{decrypt_reverse, encrypt_reverse};
use crate::utils;
use thiserror::Error;
use tracing::{Level, debug, info, span};
use umbra_types::payload::ToPayload;
use umbra_types::payload::types::TaggedPayload;
use umbra_types::{ChatMessage, Message, ToFrame, encrypted_bytes::Reversed};

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

pub trait Query {}
pub trait Store {}

pub struct InMemMsgStore {}

impl Query for InMemMsgStore {}
impl Store for InMemMsgStore {}

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

pub struct ConversationSession {
    id: String,
    owner: Identity,
    participants: Identity,
}

impl ConversationSession {
    fn new(id: String, owner: Identity, participants: Identity) -> Self {
        Self {
            id,
            owner,
            participants,
        }
    }

    fn encrypt(&self, msg: Message2) -> umbra_types::EncryptedBytes {
        let mut buf = msg.to_frame(None).encode_to_vec();
        let encrypted_bytes = encrypt_reverse(buf);
        umbra_types::EncryptedBytes {
            algo: Some(umbra_types::encrypted_bytes::Algo::Reversed(Reversed {
                encrypted_bytes,
            })),
        }
    }
}

pub struct ConversationGroup {}

pub struct UmbraClient<'a> {
    ident: Identity,
    ds: &'a dyn Publish,
    store: &'a dyn Store,

    handlers_on_conversation: Vec<Box<dyn Fn(ConversationSession) + Send + Sync>>,
    handlers_on_conversation_update: Vec<Box<dyn Fn(String) + Send + Sync>>,
    handlers_on_mesage: Vec<Box<dyn Fn(ChatMessage) + Send + Sync>>,
}

impl<'a> UmbraClient<'a> {
    fn new(ident: Identity, ds: &'a dyn Publish, store: &'a dyn Store) -> Self {
        info!(ident = ident.address(), "Client created");

        Self {
            ident,
            ds,
            store,
            handlers_on_conversation: vec![],
            handlers_on_conversation_update: vec![],
            handlers_on_mesage: vec![],
        }
    }

    pub fn create_with_ephemeral_identity(
        ds: &'a dyn Publish,
        store: &'a dyn Store,
    ) -> Result<Self, UmbraError> {
        let identity = Identity::new_ephemeral();
        Ok(Self::new(identity, ds, store))
    }

    pub fn address(&self) -> &str {
        self.ident.address()
    }

    pub fn create_session(participant: Identity) -> Result<ConversationSession, UmbraError> {
        todo!()
    }

    pub fn create_group(participants: &[Identity]) -> Result<ConversationGroup, UmbraError> {
        todo!()
    }

    pub fn send_message(
        &self,
        convo: ConversationSession,
        msg: ChatMessage,
    ) -> Result<(), UmbraError> {
        let topic = format!("inbox/{}", convo.id);
        self.ds.publish(topic, convo.encrypt(msg).to_payload());
        Ok(())
    }

    fn list_conversations(&self) -> impl Iterator<Item = ConversationSession> {
        return vec![].into_iter();
    }

    fn on_new_conversation<F>(&mut self, callback: F)
    where
        F: Fn(ConversationSession) + Send + Sync + 'static,
    {
        self.handlers_on_conversation.push(Box::new(callback));
    }

    fn on_conversation_update<F>(&mut self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.handlers_on_conversation_update
            .push(Box::new(callback));
    }
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
