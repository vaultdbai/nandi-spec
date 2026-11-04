//! One device's keys: an Ed25519 signing key and a separate X25519 sealing
//! key, never derived from each other.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hpke::kem::{Kem, X25519HkdfSha256};
use hpke::rand_core::CryptoRng;
use hpke::{Deserializable, Serializable};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{decode_array, encode, PublicKeys, SealError};

pub(crate) type SealingSecret = <X25519HkdfSha256 as Kem>::PrivateKey;
pub(crate) type SealingPublic = <X25519HkdfSha256 as Kem>::PublicKey;

/// Bytes in [`DeviceKeys::export_secret`]: the Ed25519 seed, then the
/// X25519 secret
const SECRET_BYTES: usize = 64;

/// One instance's secret keys. Kept in the OS keyring as
/// [`DeviceKeys::export_secret`]; never sent anywhere.
pub struct DeviceKeys {
    signing: SigningKey,
    sealing: SealingSecret,
    public: PublicKeys,
}

impl std::fmt::Debug for DeviceKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKeys")
            .field("public", &self.public)
            .finish_non_exhaustive()
    }
}

impl DeviceKeys {
    /// New keys from the system's randomness
    pub fn generate() -> Result<Self, SealError> {
        let mut secret = Zeroizing::new([0u8; SECRET_BYTES]);
        getrandom::fill(secret.as_mut()).map_err(|_| SealError::Randomness)?;
        Self::from_secret_bytes(&secret)
    }

    /// New keys from `rng` (tests and callers with their own source)
    pub fn generate_with_rng(rng: &mut impl CryptoRng) -> Self {
        let mut secret = Zeroizing::new([0u8; SECRET_BYTES]);
        rng.fill_bytes(secret.as_mut());
        Self::from_secret_bytes(&secret).expect("any 64 bytes are a valid secret")
    }

    /// The keys [`DeviceKeys::export_secret`] wrote
    pub fn import_secret(text: &str) -> Result<Self, SealError> {
        let secret = Zeroizing::new(decode_array::<SECRET_BYTES>(text.trim(), "the secret")?);
        Self::from_secret_bytes(&secret)
    }

    /// Both secrets, unpadded base64url, for the OS keyring
    pub fn export_secret(&self) -> Zeroizing<String> {
        let mut secret = Zeroizing::new([0u8; SECRET_BYTES]);
        secret[..32].copy_from_slice(&self.signing.to_bytes());
        secret[32..].copy_from_slice(&self.sealing.to_bytes());
        Zeroizing::new(encode(secret.as_ref()))
    }

    fn from_secret_bytes(secret: &[u8; SECRET_BYTES]) -> Result<Self, SealError> {
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.copy_from_slice(&secret[..32]);
        let signing = SigningKey::from_bytes(&seed);
        let sealing = SealingSecret::from_bytes(&secret[32..])
            .map_err(|_| SealError::BadKey("the sealing secret".into()))?;
        let sealing_public = X25519HkdfSha256::sk_to_pk(&sealing);
        let public = PublicKeys {
            signing: encode(signing.verifying_key().as_bytes()),
            sealing: encode(&sealing_public.to_bytes()),
        };
        Ok(Self {
            signing,
            sealing,
            public,
        })
    }

    /// What others pin and seal to
    pub fn public_keys(&self) -> &PublicKeys {
        &self.public
    }

    /// [`key_id`] of [`DeviceKeys::public_keys`]
    pub fn key_id(&self) -> String {
        key_id(&self.public).expect("own keys are well formed")
    }

    /// An Ed25519 signature over `message`, unpadded base64url. The seal
    /// signs its content with this (see the README for the message).
    pub fn sign(&self, message: &[u8]) -> String {
        encode(&self.signing.sign(message).to_bytes())
    }

    pub(crate) fn sign_bytes(&self, message: &[u8]) -> [u8; 64] {
        self.signing.sign(message).to_bytes()
    }

    pub(crate) fn sealing_secret(&self) -> &SealingSecret {
        &self.sealing
    }
}

/// The key ID of `keys`: the first 16 bytes of SHA-256 over the signing
/// key's 32 bytes then the sealing key's 32 bytes, as 32 lowercase hex
/// characters
pub fn key_id(keys: &PublicKeys) -> Result<String, SealError> {
    let signing = decode_array::<32>(&keys.signing, "the signing key")?;
    let sealing = decode_array::<32>(&keys.sealing, "the sealing key")?;
    let digest = Sha256::new()
        .chain_update(signing)
        .chain_update(sealing)
        .finalize();
    Ok(digest[..16].iter().map(|b| format!("{b:02x}")).collect())
}

/// A short fingerprint people compare: the key ID's first 16 characters,
/// upper case, in groups of four (`3F2A 91C0 7B44 E1D9`)
pub fn fingerprint(keys: &PublicKeys) -> Result<String, SealError> {
    let id = key_id(keys)?.to_uppercase();
    Ok(id.as_bytes()[..16]
        .chunks(4)
        .map(|group| String::from_utf8_lossy(group).into_owned())
        .collect::<Vec<_>>()
        .join(" "))
}

/// Checks `signature` (unpadded base64url) over `message` against the
/// signing key in `keys` (strict Ed25519)
pub fn verify(keys: &PublicKeys, message: &[u8], signature: &str) -> Result<(), SealError> {
    let bytes = decode_array::<64>(signature, "the signature")?;
    verify_bytes(keys, message, &bytes)
}

pub(crate) fn verify_bytes(
    keys: &PublicKeys,
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), SealError> {
    let key = signing_key(keys)?;
    let signature = Signature::from_bytes(signature);
    key.verify_strict(message, &signature)
        .map_err(|_| SealError::BadSignature)
}

fn signing_key(keys: &PublicKeys) -> Result<VerifyingKey, SealError> {
    let bytes = decode_array::<32>(&keys.signing, "the signing key")?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| SealError::BadKey("the signing key".into()))
}

/// The X25519 key envelopes for `keys` are sealed to
pub(crate) fn sealing_key(keys: &PublicKeys) -> Result<SealingPublic, SealError> {
    let bytes = decode_array::<32>(&keys.sealing, "the sealing key")?;
    SealingPublic::from_bytes(&bytes).map_err(|_| SealError::BadKey("the sealing key".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::CountingRng;

    pub(crate) fn alice() -> DeviceKeys {
        DeviceKeys::generate_with_rng(&mut CountingRng::new("alice"))
    }

    #[test]
    fn known_keys_have_a_known_id_and_fingerprint() {
        let keys = alice();
        // Pinned: a change here changes every key ID people compare
        assert_eq!(
            keys.public_keys(),
            &PublicKeys {
                signing: KAT_SIGNING.into(),
                sealing: KAT_SEALING.into(),
            }
        );
        assert_eq!(keys.key_id(), KAT_KEY_ID);
        assert_eq!(keys.key_id().len(), 32);
        let short = fingerprint(keys.public_keys()).unwrap();
        assert_eq!(short.len(), 19);
        assert_eq!(short.replace(' ', ""), KAT_KEY_ID[..16].to_uppercase());
    }

    #[test]
    fn the_secret_round_trips_and_signing_and_sealing_keys_differ() {
        let keys = alice();
        let again = DeviceKeys::import_secret(&keys.export_secret()).unwrap();
        assert_eq!(again.public_keys(), keys.public_keys());
        assert_ne!(keys.public_keys().signing, keys.public_keys().sealing);
        assert!(keys.public_keys().is_well_formed());
        assert!(matches!(
            DeviceKeys::import_secret("short"),
            Err(SealError::Malformed(_))
        ));
        let fresh = DeviceKeys::generate().unwrap();
        assert_ne!(fresh.public_keys(), keys.public_keys());
    }

    #[test]
    fn signatures_verify_only_for_their_message_and_key() {
        let keys = alice();
        let signature = keys.sign(b"hello");
        // Ed25519 is deterministic
        assert_eq!(signature, KAT_SIGNATURE);
        verify(keys.public_keys(), b"hello", &signature).unwrap();
        assert_eq!(
            verify(keys.public_keys(), b"hellp", &signature),
            Err(SealError::BadSignature)
        );
        let bob = DeviceKeys::generate_with_rng(&mut CountingRng::new("bob"));
        assert_eq!(
            verify(bob.public_keys(), b"hello", &signature),
            Err(SealError::BadSignature)
        );
        let mut flipped = crate::decode(&signature, "s").unwrap();
        flipped[10] ^= 1;
        assert_eq!(
            verify(keys.public_keys(), b"hello", &encode(&flipped)),
            Err(SealError::BadSignature)
        );
        assert!(matches!(
            verify(keys.public_keys(), b"hello", "nope"),
            Err(SealError::Malformed(_))
        ));
    }

    const KAT_SIGNING: &str = "onO1BGmfo0u2K4w6mwMD5HHd-0tgIGTQLOpdSswHQjE";
    const KAT_SEALING: &str = "-lV1n8Jzz1rGdvdCnatJRiKCr89cem5CK2ajgk_V8EI";
    const KAT_KEY_ID: &str = "390fdb1d4c4c494f106b08f376f70f0c";
    const KAT_SIGNATURE: &str =
        "Lyu5_93-EcOnPLgd-wmA8_ZI-5JZ4kn3ZPybs0dsNfzX9NWV9DjEFvfeorJ-GNS6fPivlPqc0OLBgdYcBiK8Aw";
}
