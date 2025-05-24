mod crypto;
mod utils;

use std::{cell::RefCell, collections::HashMap};
use umbra_types::Message;
use umbra_types::encrypted_bytes::Reversed;
use umbra_types::frame::FrameType;
use umbra_types::payload::EncryptedBytes;
use umbra_types::payload::types::{MetaFrameV1, SignedApplicationFrameV1, TaggedPayload};
use umbra_types::payload::{ChatMessage, ToPayload};
use umbra_types::{Frame, ToFrame};
use utils::generate_random_string;

type Addr = String;
type AddrRef<'a> = &'a str;
type Blob = Vec<u8>;
type ClientId = String;
type ClientIdRef<'a> = &'a str;
type ContentTopic = String;
type ContentTopicRef<'a> = &'a str;

use tracing::{Level, debug, info, span};

trait Publish {
    fn publish(&self, topic: ContentTopic, value: Blob);
}

//To be replaced by Subscribe
trait Poll {
    fn poll(&self, client_id: ClientIdRef, topic: &str) -> Vec<Vec<u8>>;
}

// This is hacky trait for testing
trait PrintInternals {
    fn print_internals(&self);
}

trait DistributionService: Publish + Poll + PrintInternals {}
impl<T: Publish + Poll + PrintInternals> DistributionService for T {}

// This object uses interior mutability for simplicity in single threaded code
struct LocalDistributionService {
    datastore: RefCell<HashMap<ContentTopic, Vec<Blob>>>, // Stores messages by topic
    client_cursors: RefCell<HashMap<ClientId, usize>>, // Tracks which message was last read by each client
}

impl LocalDistributionService {
    // Constructor for LocalDistributionService
    fn new() -> Self {
        Self {
            datastore: RefCell::new(HashMap::new()),
            client_cursors: RefCell::new(HashMap::new()),
        }
    }

    fn get_cursor(&self, client_id: ClientIdRef) -> usize {
        *self.client_cursors.borrow().get(client_id).unwrap_or(&0)
    }

    fn update_cursor(&self, client_id: ClientIdRef, cursor: usize) {
        self.client_cursors
            .borrow_mut()
            .insert(client_id.into(), cursor);
    }
}

impl Publish for LocalDistributionService {
    fn publish(&self, topic: ContentTopic, value: Blob) {
        let mut ds = self.datastore.borrow_mut();
        debug!("Publish [{}] {:?}", topic, value);
        ds.entry(topic).or_insert_with(Vec::new).push(value);
    }
}

impl Poll for LocalDistributionService {
    // TODO: Remove polling for PubSub
    fn poll(&self, client_id: ClientIdRef, topic: ContentTopicRef) -> Vec<Vec<u8>> {
        let start_index = self.get_cursor(client_id);

        let store = self.datastore.borrow();
        let all_msgs = store.get(topic);
        let x = all_msgs.unwrap();

        //TODO: delete
        let mut y: Vec<Vec<u8>> = x.clone();
        let z: Vec<Vec<u8>> = y.drain(start_index..y.len()).collect();

        self.update_cursor(client_id, x.len());
        return z;
    }
}

impl PrintInternals for LocalDistributionService {
    fn print_internals(&self) {
        println!("==============Datastore contents=============");
        let ds = self.datastore.borrow();
        for (topic, messages) in ds.iter() {
            println!("Topic: {}", topic);
            for message in messages {
                println!("  Message: {:?}", message);
            }
        }
        println!("==============End Store contents=============");
    }
}

struct Umbra<'a, D>
where
    D: DistributionService,
{
    identity: Addr,
    distribution_service: &'a D,
}

impl<'a, D> Umbra<'a, D>
where
    D: DistributionService,
{
    fn new(identity: Addr, distribution_service: &'a D) -> Self {
        Self {
            identity,
            distribution_service,
        }
    }

    fn address(&self) -> AddrRef {
        self.identity.as_str()
    }

    // Sends an encrypted message
    fn send_message(&self, addr: AddrRef, message: &str) {
        info!(
            from = self.address(),
            to = addr,
            content = message,
            "Sending message"
        );

        // TODO: Calculate topic after Convo type is added
        let topic = addr;

        //Encode message

        let m = ChatMessage {
            text: message.into(),
            message_id: generate_random_string(),
        };

        let frame = m.to_frame(None);
        let payload = self.encrypt(frame).to_payload();

        self.publish_to_ds(topic, payload.encode_to_vec());
    }

    fn publish_to_ds(&self, topic: ContentTopicRef, blob: Blob) {
        self.distribution_service.publish(topic.into(), blob);
    }

    fn check_for_messages(&self) {
        let span = span!(
            Level::INFO,
            "check_for_messages",
            topic = self.topic_inbox()
        );
        let _enter = span.enter();

        let topic = self.topic_inbox();
        let raw_payloads = self.distribution_service.poll(self.address(), topic);

        for raw_payload in raw_payloads {
            let tagged_payload = match TaggedPayload::decode(raw_payload.as_slice()) {
                Err(_e) => {
                    debug!("Payload was decodable");
                    continue;
                }
                Ok(payload) => payload,
            };

            info!(
                proto = tagged_payload.protocol,
                tag = tagged_payload.tag,
                len = tagged_payload.payload_bytes.len(),
                "Payload received",
            );

            self.handle_tagged_payload(tagged_payload);
        }
    }

    fn handle_tagged_payload(&self, tagged_payload: TaggedPayload) {
        let tag = tagged_payload.tag;

        // TODO: This needs to provide some safety to catching meta messages but also handle developer defined packaget

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
        };

        if let Some(f) = frame {
            self.handle_frame(f);
        }
    }

    fn topic_inbox(&self) -> ContentTopicRef {
        self.address()
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

    fn handle_frame(&self, frame: Frame) {
        let _ = handle_reliability_info(frame.reliability_info);

        match frame.frame_type.ok_or_else(|| 0).unwrap() {
            FrameType::PublicFrameFrame(public_frame) => {
                debug!("PublicFrame: {:?}", public_frame);
                match public_frame.frame_type {
                    Some(umbra_types::public_frame::FrameType::Contact(contact)) => {
                        info!("Contact: {:?}", contact);
                    }
                    _ => {}
                }
            }
            FrameType::ConfidentialFrame(confidential_frame) => {
                debug!("ConfidentialFrame: {:?}", confidential_frame);
                match confidential_frame.r#type {
                    Some(umbra_types::confidential_frame::Type::AppFrameV1(frame)) => {
                        debug!("AppFrameV1: {:?}", frame);
                        self.handle_content_frame(frame);
                    }
                    Some(umbra_types::confidential_frame::Type::SignedFrame(frame)) => {
                        debug!("SignedFrame: {:?}", frame);
                        self.handle_signed_frame(frame);
                    }
                    Some(umbra_types::confidential_frame::Type::MetaFrame(frame)) => {
                        debug!("MetaFrame: {:?}", frame);
                        self.handle_meta_frame(frame);
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_content_frame(&self, frame: umbra_types::ApplicationFrameV1) {
        match frame.payload.unwrap() {
            umbra_types::application_frame_v1::Payload::ChatMsg(chat_message) => {
                info!("ChatMessage: {:?}", chat_message);
            }
            umbra_types::application_frame_v1::Payload::DeveloperSpecified(items) => {
                info!("DeveloperSpecified: {:?}", items);
            }
        }
    }

    fn handle_signed_frame(&self, _frame: SignedApplicationFrameV1) {
        todo!()
    }

    fn handle_meta_frame(&self, _frame: MetaFrameV1) {
        todo!()
    }
}

fn handle_reliability_info(_reliability_info: Option<umbra_types::ReliabilityInfo>) {
    // todo!()
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // Set the maximum log level
        .with_target(false)
        .init();

    let span = span!(Level::INFO, "Umbra POC",);
    let _enter = span.enter();

    info!("Starting Umbra POC");

    let ds = LocalDistributionService::new();

    let amal = Umbra::new("amal".into(), &ds);
    let bola = Umbra::new("bola".into(), &ds);

    amal.send_message(bola.address(), "Hello, World!");
    amal.send_message(bola.address(), "Bye!");

    bola.check_for_messages();
    bola.send_message(amal.address(), "My name is not world");
    bola.check_for_messages();

    amal.check_for_messages();

    ds.print_internals();
}
