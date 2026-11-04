//! Nandi's end-to-end seal (`nandi-seal-v1`): per-device keys, content
//! sealed for one recipient, and signatures over that content. Pure: no
//! I/O beyond the system's randomness for new keys and ephemeral keys.
//!
//! - [`DeviceKeys`]: one instance's secret keys, an Ed25519 signing key
//!   and a separate X25519 sealing key. [`key_id`] and [`fingerprint`]
//!   name its [`PublicKeys`].
//! - [`seal`] and [`open`]: HPKE (RFC 9180) base mode,
//!   DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305. The
//!   envelope's [`SealedFor`] and both key IDs are the associated data, so
//!   a relay can't move a blob to another task, sequence number, person,
//!   purpose or capability. Inside, the content is signed by its author.
//! - [`reseal`]: the provider of a reassigned task seals the opened
//!   content for the new provider, keeping the author's signature.
//!
//! The wire shapes (`PublicKeys`, `Envelope`, `SealedFor`) live in
//! `nandi-protocol`, so a relay carries envelopes without linking this
//! crate. The construction is in the README.

mod error;
mod keys;
mod seal;

pub use error::SealError;
pub use keys::{fingerprint, key_id, verify, DeviceKeys};
pub use nandi_protocol::{Envelope, PublicKeys, SealPurpose, SealedFor, SEAL_SUITE};
pub use seal::{open, reseal, seal, seal_with_rng, Opened};

/// The randomness traits [`DeviceKeys::generate_with_rng`] and
/// [`seal_with_rng`] take (the `rand_core` hpke uses)
pub use hpke::rand_core;

/// Unpadded base64url, the encoding of every key and byte string here
pub(crate) fn encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn decode(text: &str, what: &str) -> Result<Vec<u8>, SealError> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| SealError::Malformed(format!("{what} isn't unpadded base64url")))
}

/// `text` decoded into exactly `N` bytes
pub(crate) fn decode_array<const N: usize>(text: &str, what: &str) -> Result<[u8; N], SealError> {
    decode(text, what)?
        .try_into()
        .map_err(|_| SealError::Malformed(format!("{what} isn't {N} bytes")))
}

#[cfg(test)]
pub(crate) mod testing {
    use hpke::rand_core::{Infallible, TryCryptoRng, TryRng};

    /// A deterministic "random" source for known-answer tests: SHA-256 in
    /// counter mode over a label. Never for real keys.
    pub struct CountingRng {
        label: Vec<u8>,
        counter: u64,
    }

    impl CountingRng {
        pub fn new(label: &str) -> Self {
            Self {
                label: label.as_bytes().to_vec(),
                counter: 0,
            }
        }
    }

    impl TryRng for CountingRng {
        type Error = Infallible;

        fn try_next_u32(&mut self) -> Result<u32, Infallible> {
            let mut bytes = [0; 4];
            self.try_fill_bytes(&mut bytes)?;
            Ok(u32::from_le_bytes(bytes))
        }

        fn try_next_u64(&mut self) -> Result<u64, Infallible> {
            let mut bytes = [0; 8];
            self.try_fill_bytes(&mut bytes)?;
            Ok(u64::from_le_bytes(bytes))
        }

        fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
            use sha2::{Digest, Sha256};
            for chunk in dst.chunks_mut(32) {
                let block = Sha256::new()
                    .chain_update(&self.label)
                    .chain_update(self.counter.to_be_bytes())
                    .finalize();
                self.counter += 1;
                chunk.copy_from_slice(&block[..chunk.len()]);
            }
            Ok(())
        }
    }

    impl TryCryptoRng for CountingRng {}
}
