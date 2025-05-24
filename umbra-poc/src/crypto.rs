pub fn encrypt_reverse(mut buf: Vec<u8>) -> Vec<u8> {
    buf.reverse();
    buf
}

pub fn decrypt_reverse(buf: Vec<u8>) -> Vec<u8> {
    encrypt_reverse(buf)
}
