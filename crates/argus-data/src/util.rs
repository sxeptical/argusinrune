//! Stable short ids derived from upstream rows (the TS clients used
//! `createHash("sha256").update(...).digest("hex").slice(0, 16)`).

use sha2::{Digest, Sha256};

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn short_hash(input: &str) -> String {
    sha256_hex(input)[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_16_hex_chars() {
        let id = short_hash("Accident|1.3|103.8");
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(id, short_hash("Accident|1.3|103.8"));
        assert_ne!(id, short_hash("Obstacle|1.3|103.8"));
    }
}
