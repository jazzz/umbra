use rand::{self, Rng, distr::Alphanumeric};

pub fn generate_random_string() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(8) // Take 8 characters
        .map(char::from) // Convert each byte to a char
        .collect() // Collect into a String
}
