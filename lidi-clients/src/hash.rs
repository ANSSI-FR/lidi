use twox_hash::xxhash3_128;

#[derive(Default)]
pub struct StreamHasher(xxhash3_128::Hasher);

impl StreamHasher {
    pub fn update(&mut self, buf: &[u8]) {
        self.0.write(buf);
    }

    #[must_use]
    pub fn finalize(&self) -> u128 {
        self.0.finish_128()
    }
}
