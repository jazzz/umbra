use rand::{self, Rng, distr::Alphanumeric};

pub fn generate_random_string(length: u8) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length as usize) // Take 8 characters
        .map(char::from) // Convert each byte to a char
        .collect() // Collect into a String
}
