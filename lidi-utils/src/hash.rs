use std::hash::Hasher;
use twox_hash::xxh3::{self, HasherExt};

#[derive(Default)]
pub struct StreamHasher(xxh3::Hash128);

impl StreamHasher {
    pub fn update(&mut self, buf: &[u8]) {
        self.0.write(buf);
    }

    #[must_use]
    pub fn finalize(&self) -> u128 {
        self.0.finish_ext()
    }
}
