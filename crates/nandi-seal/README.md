# nandi-seal

Nandi's end-to-end seal, `nandi-seal-v1`: per-device keys, content sealed for one recipient, and the author's signature inside. Pure Rust, no I/O (only the system's randomness, for new keys and ephemeral keys). MIT licensed.

The wire shapes (`PublicKeys`, `Envelope`, `SealedFor`, `SealPurpose`) are in `nandi-protocol`. A relay carries envelopes with those types and never links this crate.

| Item                         | What                                                               |
| ---------------------------- | ------------------------------------------------------------------ |
| `DeviceKeys`                 | One instance's secret keys; `export_secret` / `import_secret`      |
| `key_id`, `fingerprint`      | Name a `PublicKeys`: 32 hex characters, or `3F2A 91C0 7B44 E1D9`   |
| `DeviceKeys::sign`, `verify` | Ed25519 over raw bytes                                             |
| `seal`, `seal_with_rng`      | Seal content for a recipient, signed by the sender as author       |
| `open`                       | Open an envelope and check its author's signature                  |
| `reseal`                     | Hand opened content to a new recipient, keeping the author's sig   |
| `SealError`                  | Which check failed, and nothing more                               |

## Keys

Each device (instance) has two separate keys. Neither is derived from the other.

| Key     | Algorithm | Used for                       | Public form                 |
| ------- | --------- | ------------------------------ | --------------------------- |
| Signing | Ed25519   | The author's signature         | 32 bytes, unpadded base64url |
| Sealing | X25519    | HPKE recipient key (KEM)       | 32 bytes, unpadded base64url |

- **Secret:** 64 bytes, the Ed25519 seed then the X25519 secret, as unpadded base64url (86 characters). It lives in the OS keyring only.
- **Key ID:** the first 16 bytes of `SHA-256(signing public key ‖ sealing public key)` (the 32 raw bytes of each), as 32 lowercase hex characters.
- **Fingerprint:** the key ID's first 16 characters, upper case, in groups of four. People compare it when a key changes.

## The construction

Every length-prefixed field (`lp`) is a 4-byte big-endian length, then the bytes. Strings are UTF-8. `seq` is 8 bytes big endian.

**HPKE** (RFC 9180), the `hpke` crate 0.15:

| Parameter | Value                                        |
| --------- | -------------------------------------------- |
| Mode      | Base (`mode_base`, no sender authentication) |
| KEM       | DHKEM(X25519, HKDF-SHA256), `0x0020`         |
| KDF       | HKDF-SHA256, `0x0001`                        |
| AEAD      | ChaCha20-Poly1305, `0x0003`                  |
| `info`    | `nandi-seal-v1`                              |

**Associated data** binds the envelope's `SealedFor` and both key IDs:

```
lp("nandi-seal-v1 aad") ‖ lp(task_id) ‖ lp(seq as u64 BE) ‖ lp(from) ‖ lp(to)
  ‖ lp(purpose) ‖ lp(capability) ‖ lp(sender_key_id) ‖ lp(recipient_key_id)
```

`purpose` is its wire name (`request`, `result`, `clarification`, `answer`, `note`).

**Signed message.** The author signs, with Ed25519:

```
lp("nandi-seal-v1 content") ‖ lp(task_id) ‖ lp(purpose) ‖ lp(capability) ‖ content
```

It leaves out `seq`, `from` and `to`, so a reseal for a reassigned task keeps the requester's signature.

**Plaintext** sealed by HPKE:

```
author key ID (16 raw bytes) ‖ Ed25519 signature (64 bytes) ‖ content
```

**Envelope:** `suite` = `nandi-seal-v1`, the `SealedFor` in the clear, `senderKeyId`, `recipientKeyId`, `enc` (the 32-byte encapsulated ephemeral key) and `ciphertext` (with its 16-byte tag), bytes as unpadded base64url.

## Opening

`open(recipient, envelope, author)` fails unless every check passes, in this order:

1. `suite` is `nandi-seal-v1` (else `UnknownSuite`).
2. `recipientKeyId` is the recipient's key ID (else `WrongRecipient`).
3. HPKE opens with the associated data rebuilt from the envelope (else `NotOpened`: a changed ciphertext, `enc`, `SealedFor` field or key ID).
4. The author key ID inside is `author`'s and the signature verifies, strict Ed25519 (else `BadSignature`).

The caller passes the author it expects (the keys it pinned for that person), so a relay can't swap who asked.

## Limits

- Base mode doesn't authenticate the sender: the signature inside does. Anyone may seal to a public key, but only the author's signature makes the content theirs.
- Static recipient keys: a stolen device key opens envelopes still stored for it. Keep retention short.
- The relay sees `SealedFor` and the key IDs by design (it routes by them).
- Replays: the receiver drops a `(task_id, from, seq)` it has already opened. `seq` is unique among one sender's envelopes for a task.
- Size: an envelope is at most `MAX_ENVELOPE_BYTES` (1 MiB, `nandi-protocol`). Files travel as links.

What each envelope holds, per message, is in `nandi-protocol` (`content`, and its README's "Sealing" table).

## Tests

`cargo test -p nandi-seal`: known answers (keys, key ID, signature and one envelope from a deterministic source), tampering with the ciphertext, `enc`, every `SealedFor` field, either key ID and the signature, the wrong recipient, a forged author, and reseal. The `hpke` crate checks itself against the RFC 9180 vectors.
