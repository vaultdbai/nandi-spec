//! Sealing content for one recipient and opening it (see the README for
//! the exact bytes).

use hpke::aead::ChaCha20Poly1305;
use hpke::kdf::HkdfSha256;
use hpke::kem::{Kem, X25519HkdfSha256};
use hpke::rand_core::CryptoRng;
use hpke::{Deserializable, OpModeR, OpModeS, Serializable};

use crate::keys::{key_id, sealing_key, verify_bytes};
use crate::{decode, decode_array, encode, DeviceKeys, Envelope, PublicKeys, SealError, SealedFor};
use crate::{SealPurpose, SEAL_SUITE};

type EncappedKey = <X25519HkdfSha256 as Kem>::EncappedKey;

/// HPKE's `info`: the suite name
const INFO: &[u8] = b"nandi-seal-v1";
/// The first field of the associated data
const AAD_LABEL: &[u8] = b"nandi-seal-v1 aad";
/// The first field of the message the author signs
const SIGNED_LABEL: &[u8] = b"nandi-seal-v1 content";

/// Bytes before the content in the sealed plaintext: the author's key ID
/// (16) and their signature (64)
const HEADER_BYTES: usize = 16 + 64;

/// An opened envelope
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub sealed_for: SealedFor,
    pub content: Vec<u8>,
    /// Who signed the content (the requester, after a reseal)
    pub author_key_id: String,
    signature: [u8; 64],
}

/// Seals `content` for `recipient`, signed by `sender` as its author, with
/// the system's randomness
pub fn seal(
    sender: &DeviceKeys,
    recipient: &PublicKeys,
    sealed_for: SealedFor,
    content: &[u8],
) -> Result<Envelope, SealError> {
    let mut rng = SystemRng;
    seal_with_rng(sender, recipient, sealed_for, content, &mut rng)
}

/// [`seal`] with the ephemeral key drawn from `rng` (known-answer tests)
pub fn seal_with_rng(
    sender: &DeviceKeys,
    recipient: &PublicKeys,
    sealed_for: SealedFor,
    content: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<Envelope, SealError> {
    let signature = sender.sign_bytes(&signed_message(&sealed_for, content));
    let author = key_id_bytes(&sender.key_id())?;
    seal_signed(
        sender, recipient, sealed_for, author, signature, content, rng,
    )
}

/// Seals what `opened` holds for `recipient`, sent by `sender` (the
/// provider handing a task on). The author's signature is kept, so the
/// new provider can check who asked. The task, purpose and capability
/// can't change.
pub fn reseal(
    sender: &DeviceKeys,
    opened: &Opened,
    recipient: &PublicKeys,
    sealed_for: SealedFor,
) -> Result<Envelope, SealError> {
    let before = &opened.sealed_for;
    if (
        before.task_id.as_str(),
        before.purpose,
        before.capability.as_str(),
    ) != (
        sealed_for.task_id.as_str(),
        sealed_for.purpose,
        sealed_for.capability.as_str(),
    ) {
        return Err(SealError::Rebound);
    }
    let author = key_id_bytes(&opened.author_key_id)?;
    let mut rng = SystemRng;
    seal_signed(
        sender,
        recipient,
        sealed_for,
        author,
        opened.signature,
        &opened.content,
        &mut rng,
    )
}

fn seal_signed(
    sender: &DeviceKeys,
    recipient: &PublicKeys,
    sealed_for: SealedFor,
    author: [u8; 16],
    signature: [u8; 64],
    content: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<Envelope, SealError> {
    let recipient_key = sealing_key(recipient)?;
    let sender_key_id = sender.key_id();
    let recipient_key_id = key_id(recipient)?;
    let aad = associated_data(&sealed_for, &sender_key_id, &recipient_key_id);
    let mut plaintext = Vec::with_capacity(HEADER_BYTES + content.len());
    plaintext.extend_from_slice(&author);
    plaintext.extend_from_slice(&signature);
    plaintext.extend_from_slice(content);
    let (enc, ciphertext) = hpke::single_shot_seal_with_rng::<
        ChaCha20Poly1305,
        HkdfSha256,
        X25519HkdfSha256,
    >(&OpModeS::Base, &recipient_key, INFO, &plaintext, &aad, rng)
    .map_err(|_| SealError::BadKey("the recipient's sealing key".into()))?;
    Ok(Envelope {
        suite: SEAL_SUITE.to_string(),
        sealed_for,
        sender_key_id,
        recipient_key_id,
        enc: encode(&enc.to_bytes()),
        ciphertext: encode(&ciphertext),
    })
}

/// Opens `envelope` with `recipient`'s keys and checks that `author`
/// signed its content. Fails, saying only which check, if anything in it
/// or its [`SealedFor`] was changed.
pub fn open(
    recipient: &DeviceKeys,
    envelope: &Envelope,
    author: &PublicKeys,
) -> Result<Opened, SealError> {
    if envelope.suite != SEAL_SUITE {
        return Err(SealError::UnknownSuite(envelope.suite.clone()));
    }
    if envelope.recipient_key_id != recipient.key_id() {
        return Err(SealError::WrongRecipient);
    }
    let enc = EncappedKey::from_bytes(&decode_array::<32>(&envelope.enc, "enc")?)
        .map_err(|_| SealError::Malformed("enc isn't an X25519 key".into()))?;
    let ciphertext = decode(&envelope.ciphertext, "the ciphertext")?;
    let aad = associated_data(
        &envelope.sealed_for,
        &envelope.sender_key_id,
        &envelope.recipient_key_id,
    );
    let plaintext = hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
        &OpModeR::Base,
        recipient.sealing_secret(),
        &enc,
        INFO,
        &ciphertext,
        &aad,
    )
    .map_err(|_| SealError::NotOpened)?;
    if plaintext.len() < HEADER_BYTES {
        return Err(SealError::NotOpened);
    }
    let (author_id, rest) = plaintext.split_at(16);
    let (signature, content) = rest.split_at(64);
    if author_id != key_id_bytes(&key_id(author)?)? {
        return Err(SealError::BadSignature);
    }
    let signature: [u8; 64] = signature.try_into().expect("split at 64");
    verify_bytes(
        author,
        &signed_message(&envelope.sealed_for, content),
        &signature,
    )?;
    Ok(Opened {
        sealed_for: envelope.sealed_for.clone(),
        content: content.to_vec(),
        author_key_id: hex(author_id),
        signature,
    })
}

/// The associated data: every field length-prefixed (u32 big endian),
/// `seq` as 8 bytes big endian
fn associated_data(sealed_for: &SealedFor, sender_key_id: &str, recipient_key_id: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for field in [
        AAD_LABEL,
        sealed_for.task_id.as_bytes(),
        &sealed_for.seq.to_be_bytes(),
        sealed_for.from.as_bytes(),
        sealed_for.to.as_bytes(),
        sealed_for.purpose.as_str().as_bytes(),
        sealed_for.capability.as_bytes(),
        sender_key_id.as_bytes(),
        recipient_key_id.as_bytes(),
    ] {
        push_field(&mut out, field);
    }
    out
}

/// What the author signs: the label, task ID, purpose and capability
/// (length-prefixed), then the content. Not the sequence number or the
/// people, so a reseal keeps the signature.
fn signed_message(sealed_for: &SealedFor, content: &[u8]) -> Vec<u8> {
    signed_message_parts(
        &sealed_for.task_id,
        sealed_for.purpose,
        &sealed_for.capability,
        content,
    )
}

fn signed_message_parts(
    task_id: &str,
    purpose: SealPurpose,
    capability: &str,
    content: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    for field in [
        SIGNED_LABEL,
        task_id.as_bytes(),
        purpose.as_str().as_bytes(),
        capability.as_bytes(),
    ] {
        push_field(&mut out, field);
    }
    out.extend_from_slice(content);
    out
}

fn push_field(out: &mut Vec<u8>, field: &[u8]) {
    let len = u32::try_from(field.len()).expect("fields are far below 4 GiB");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(field);
}

fn key_id_bytes(id: &str) -> Result<[u8; 16], SealError> {
    let malformed = || SealError::Malformed("a key ID isn't 32 hex characters".into());
    if id.len() != 32 {
        return Err(malformed());
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&id[i * 2..i * 2 + 2], 16).map_err(|_| malformed())?;
    }
    Ok(out)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The system's randomness as a `CryptoRng`; panics only if the OS has
/// none to give, as hpke's own does
struct SystemRng;

impl hpke::rand_core::TryRng for SystemRng {
    type Error = hpke::rand_core::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        let mut bytes = [0; 4];
        self.try_fill_bytes(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        let mut bytes = [0; 8];
        self.try_fill_bytes(&mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        getrandom::fill(dst).expect("the system's randomness failed");
        Ok(())
    }
}

impl hpke::rand_core::TryCryptoRng for SystemRng {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::CountingRng;

    fn keys(name: &str) -> DeviceKeys {
        DeviceKeys::generate_with_rng(&mut CountingRng::new(name))
    }

    fn sealed_for() -> SealedFor {
        SealedFor {
            task_id: "t1".into(),
            seq: 0,
            from: "alice-1".into(),
            to: "bob-1".into(),
            purpose: SealPurpose::Request,
            capability: "write".into(),
        }
    }

    const CONTENT: &[u8] = br#"{"topic":"the Q3 numbers"}"#;

    fn sealed() -> Envelope {
        seal_with_rng(
            &keys("alice"),
            keys("bob").public_keys(),
            sealed_for(),
            CONTENT,
            &mut CountingRng::new("ephemeral"),
        )
        .unwrap()
    }

    #[test]
    fn a_known_envelope_opens_to_its_content() {
        let envelope = sealed();
        // Pinned: the construction can't drift without this changing
        assert_eq!(envelope.enc, KAT_ENC);
        assert_eq!(envelope.ciphertext, KAT_CIPHERTEXT);
        let fixed = Envelope {
            enc: KAT_ENC.into(),
            ciphertext: KAT_CIPHERTEXT.into(),
            ..envelope
        };
        let opened = open(&keys("bob"), &fixed, keys("alice").public_keys()).unwrap();
        assert_eq!(opened.content, CONTENT);
        assert_eq!(opened.author_key_id, keys("alice").key_id());
        assert_eq!(opened.sealed_for, sealed_for());
    }

    #[test]
    fn sealing_twice_differs_but_both_open() {
        let (alice, bob) = (keys("alice"), keys("bob"));
        let one = seal(&alice, bob.public_keys(), sealed_for(), CONTENT).unwrap();
        let two = seal(&alice, bob.public_keys(), sealed_for(), CONTENT).unwrap();
        assert_ne!(one.ciphertext, two.ciphertext);
        assert_eq!(one.suite, SEAL_SUITE);
        assert_eq!(one.sender_key_id, alice.key_id());
        assert_eq!(one.recipient_key_id, bob.key_id());
        for envelope in [one, two] {
            assert_eq!(
                open(&bob, &envelope, alice.public_keys()).unwrap().content,
                CONTENT
            );
        }
        // Empty content seals too
        let empty = seal(&alice, bob.public_keys(), sealed_for(), b"").unwrap();
        assert!(open(&bob, &empty, alice.public_keys())
            .unwrap()
            .content
            .is_empty());
    }

    fn opens(envelope: &Envelope) -> Result<Opened, SealError> {
        open(&keys("bob"), envelope, keys("alice").public_keys())
    }

    #[test]
    fn a_changed_ciphertext_or_enc_doesnt_open() {
        let envelope = sealed();
        let mut bytes = decode(&envelope.ciphertext, "c").unwrap();
        for i in [0, bytes.len() / 2, bytes.len() - 1] {
            bytes[i] ^= 0x01;
            let tampered = Envelope {
                ciphertext: encode(&bytes),
                ..envelope.clone()
            };
            assert_eq!(opens(&tampered), Err(SealError::NotOpened));
            bytes[i] ^= 0x01;
        }
        let truncated = Envelope {
            ciphertext: encode(&bytes[..bytes.len() - 1]),
            ..envelope.clone()
        };
        assert_eq!(opens(&truncated), Err(SealError::NotOpened));
        let other_enc = Envelope {
            enc: keys("mallory").public_keys().sealing.clone(),
            ..envelope.clone()
        };
        assert_eq!(opens(&other_enc), Err(SealError::NotOpened));
        let bad_suite = Envelope {
            suite: "nandi-seal-v0".into(),
            ..envelope
        };
        assert!(matches!(opens(&bad_suite), Err(SealError::UnknownSuite(_))));
    }

    #[test]
    fn every_bound_field_is_checked() {
        let envelope = sealed();
        let changes: [fn(&mut SealedFor); 6] = [
            |s| s.task_id = "t2".into(),
            |s| s.seq = 1,
            |s| s.from = "mallory-1".into(),
            |s| s.to = "carol-1".into(),
            |s| s.purpose = SealPurpose::Result,
            |s| s.capability = "delete".into(),
        ];
        for change in changes {
            let mut tampered = envelope.clone();
            change(&mut tampered.sealed_for);
            assert_eq!(opens(&tampered), Err(SealError::NotOpened));
        }
    }

    #[test]
    fn a_changed_key_id_or_the_wrong_recipient_fails() {
        let envelope = sealed();
        let sender_changed = Envelope {
            sender_key_id: keys("mallory").key_id(),
            ..envelope.clone()
        };
        assert_eq!(opens(&sender_changed), Err(SealError::NotOpened));
        let recipient_changed = Envelope {
            recipient_key_id: keys("carol").key_id(),
            ..envelope.clone()
        };
        assert_eq!(opens(&recipient_changed), Err(SealError::WrongRecipient));
        // Carol can't open Bob's, even claiming it's hers
        let claimed = Envelope {
            recipient_key_id: keys("carol").key_id(),
            ..envelope
        };
        assert_eq!(
            open(&keys("carol"), &claimed, keys("alice").public_keys()),
            Err(SealError::NotOpened)
        );
    }

    #[test]
    fn content_signed_by_someone_else_is_refused() {
        // Mallory seals for Bob, claiming to be Alice: the seal opens (it's
        // sealed to Bob) but Alice didn't sign it
        let mallory = keys("mallory");
        let forged = seal(&mallory, keys("bob").public_keys(), sealed_for(), CONTENT).unwrap();
        assert_eq!(opens(&forged), Err(SealError::BadSignature));
        // A wrong signature under Alice's key ID
        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(&key_id_bytes(&keys("alice").key_id()).unwrap());
        plaintext.extend_from_slice(&[7u8; 64]);
        plaintext.extend_from_slice(CONTENT);
        let bob = keys("bob");
        let aad = associated_data(&sealed_for(), &mallory.key_id(), &bob.key_id());
        let (enc, ciphertext) =
            hpke::single_shot_seal_with_rng::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
                &OpModeS::Base,
                &sealing_key(bob.public_keys()).unwrap(),
                INFO,
                &plaintext,
                &aad,
                &mut CountingRng::new("e"),
            )
            .unwrap();
        let bad = Envelope {
            suite: SEAL_SUITE.into(),
            sealed_for: sealed_for(),
            sender_key_id: mallory.key_id(),
            recipient_key_id: bob.key_id(),
            enc: encode(&enc.to_bytes()),
            ciphertext: encode(&ciphertext),
        };
        assert_eq!(opens(&bad), Err(SealError::BadSignature));
    }

    #[test]
    fn a_reseal_keeps_the_authors_signature() {
        let (alice, bob, carol) = (keys("alice"), keys("bob"), keys("carol"));
        let opened = open(&bob, &sealed(), alice.public_keys()).unwrap();
        let onward = SealedFor {
            seq: 1,
            from: "bob-1".into(),
            to: "carol-1".into(),
            ..sealed_for()
        };
        let envelope = reseal(&bob, &opened, carol.public_keys(), onward.clone()).unwrap();
        assert_eq!(envelope.sender_key_id, bob.key_id());
        let at_carol = open(&carol, &envelope, alice.public_keys()).unwrap();
        assert_eq!(at_carol.content, CONTENT);
        assert_eq!(at_carol.author_key_id, alice.key_id());
        // Bob didn't author it
        assert_eq!(
            open(&carol, &envelope, bob.public_keys()),
            Err(SealError::BadSignature)
        );
        // Nor may he change what Alice signed
        let rebound = SealedFor {
            capability: "delete".into(),
            ..onward
        };
        assert_eq!(
            reseal(&bob, &opened, carol.public_keys(), rebound),
            Err(SealError::Rebound)
        );
    }

    #[test]
    fn malformed_envelopes_and_keys_are_refused() {
        let envelope = sealed();
        let not_base64 = Envelope {
            ciphertext: "***".into(),
            ..envelope.clone()
        };
        assert!(matches!(opens(&not_base64), Err(SealError::Malformed(_))));
        let short_enc = Envelope {
            enc: "AAAA".into(),
            ..envelope
        };
        assert!(matches!(opens(&short_enc), Err(SealError::Malformed(_))));
        let bad_recipient = PublicKeys {
            signing: keys("bob").public_keys().signing.clone(),
            sealing: "x".into(),
        };
        assert!(matches!(
            seal(&keys("alice"), &bad_recipient, sealed_for(), CONTENT),
            Err(SealError::Malformed(_))
        ));
    }

    const KAT_ENC: &str = "_37DDAwNsZ6OFxeRGkf5rB91yM4PMwaDl2kvHwkMAnU";
    const KAT_CIPHERTEXT: &str = "IFLoGIVgwHDNsn71IB_fW9W1Z3eXYc8EkeoXAQBE1-P1L-o8eod46ln7u8w_qbfONtLQSag3n7ZNxHPljv5mo55mFdNqqc6CJOdkFeBOP8k7dihIDJwvFvgd3wYrmZ4il5L4Tut8YgYKqlYNXrrxsGa_-tBpMwpRw-8";
}
