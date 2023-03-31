use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use format::{Digest, SHA256_BLOCK_SIZE};

const NAME_ANNOTATION: &str = "org.opencontainers.image.ref.name";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Descriptor {
    pub digest: Digest,
    pub size: u64,
    pub media_type: String,
    pub annotations: HashMap<String, String>,
    pub fs_verity_digest: [u8; SHA256_BLOCK_SIZE],
    pub compressed: bool,
}

impl Descriptor {
    pub fn new(
        digest: [u8; 32],
        size: u64,
        media_type: String,
        fs_verity_digest: [u8; SHA256_BLOCK_SIZE],
        compressed: bool,
    ) -> Descriptor {
        Descriptor {
            digest: Digest::new(&digest),
            size,
            media_type,
            annotations: HashMap::new(),
            fs_verity_digest,
            compressed,
        }
    }

    pub fn set_name(&mut self, name: &str) {
        self.annotations
            .insert(NAME_ANNOTATION.to_string(), name.into());
    }

    pub fn get_name(&self) -> Option<&String> {
        self.annotations.get(NAME_ANNOTATION)
    }

    pub(crate) fn remove_name(&mut self) {
        self.annotations.remove_entry(NAME_ANNOTATION);
    }
}
