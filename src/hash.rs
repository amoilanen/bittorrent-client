
use sha1::{Sha1, Digest};

pub(crate) fn compute_hash(input: &Vec<u8>) -> Vec<u8> {
    let mut hasher = Sha1::new();
    hasher.update(input);
    hasher.finalize().to_vec()
}

//TODO: Add tests