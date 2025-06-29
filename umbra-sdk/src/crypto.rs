use sha3::{Digest, Sha3_256};

pub fn encrypt_reverse(mut buf: Vec<u8>) -> Vec<u8> {
    buf.reverse();
    buf
}

pub fn decrypt_reverse(buf: Vec<u8>) -> Vec<u8> {
    encrypt_reverse(buf)
}

pub fn hash_to_string<T: AsRef<[u8]>>(buf: T) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(buf.as_ref());
    let result = hasher.finalize();
    hex::encode(result)
}
