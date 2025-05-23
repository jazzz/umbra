use std::{cell::RefCell, collections::HashMap};

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
    fn publish(&self, topic: ContentTopic, value: Vec<u8>) {
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
        let payload = message.as_bytes().to_vec();

        self.distribution_service.publish(topic.into(), payload);
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

        for payload in raw_payloads {
            //Decode Message
            let message = String::from_utf8(payload).unwrap();
            info!("Received message: {}", message);
        }
    }

    fn topic_inbox(&self) -> ContentTopicRef {
        self.address()
    }
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
