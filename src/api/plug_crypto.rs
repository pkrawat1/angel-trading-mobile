//! Reads values the web app stores with `Plug.Crypto.encrypt/4`. Key
//! derivation and both token formats follow plug_crypto 2.1.1, the version the
//! web app locks, which still reads the AES-GCM tokens plug_crypto 1.x wrote.

use std::num::NonZeroU32;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::pbkdf2;

use super::etf::{self, Term};

// plug_crypto authenticates every token with this string, a leftover from its
// original A128GCM format.
const AAD: &[u8] = b"A128GCM";

/// The term given to `Plug.Crypto.encrypt(key_base, context, term, opts)`, or
/// `None` when the token is malformed or the key or context is wrong.
pub fn decrypt(key_base: &str, context: &str, token: &str) -> Option<Term> {
    let key = derive_key(key_base, context);
    let plain = if let Some(rest) = token.strip_prefix("XCP.") {
        open_xchacha20(&key, &URL_SAFE_NO_PAD.decode(rest).ok()?)?
    } else if let Some(rest) = token.strip_prefix("QTEyOEdDTQ.") {
        open_legacy(&key, rest)?
    } else {
        return None;
    };
    unwrap_signed(etf::decode(&plain)?)
}

/// `Plug.Crypto.KeyGenerator` with the defaults `Plug.Crypto.encrypt/4` uses:
/// PBKDF2-HMAC-SHA256 over 1000 iterations, salted with the context.
fn derive_key(key_base: &str, context: &str) -> [u8; 32] {
    let mut key = [0; 32];
    let iterations = NonZeroU32::new(1000).expect("non-zero");
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        context.as_bytes(),
        key_base.as_bytes(),
        &mut key,
    );
    key
}

/// plug_crypto 2.x: XChaCha20-Poly1305, laid out as nonce, tag, cipher text.
fn open_xchacha20(key: &[u8; 32], raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() < 24 + 16 {
        return None;
    }
    let (nonce, rest) = raw.split_at(24);
    let (tag, cipher_text) = rest.split_at(16);
    let subkey = hchacha20(key, nonce[..16].try_into().ok()?);
    let mut short_nonce = [0; 12];
    short_nonce[4..].copy_from_slice(&nonce[16..]);
    // ring expects the tag after the cipher text.
    let mut sealed = [cipher_text, tag].concat();
    open(&aead::CHACHA20_POLY1305, &subkey, short_nonce, AAD, &mut sealed)
}

/// plug_crypto 1.x: a random AES-GCM content key, wrapped with AES-256-GCM
/// under the derived key (RFC 7518, section 4.7).
fn open_legacy(key: &[u8; 32], rest: &str) -> Option<Vec<u8>> {
    let parts = rest
        .split('.')
        .map(|part| URL_SAFE_NO_PAD.decode(part).ok())
        .collect::<Option<Vec<_>>>()?;
    let [wrapped, iv, cipher_text, tag] = parts.as_slice() else {
        return None;
    };
    // The wrapped key is cipher text, then tag, then a 12-byte IV.
    let (sealed_key, wrap_iv) = wrapped.split_at(wrapped.len().checked_sub(12)?);
    let content_key = open(
        &aead::AES_256_GCM,
        key,
        wrap_iv.try_into().ok()?,
        b"",
        &mut sealed_key.to_vec(),
    )?;
    let algorithm = match content_key.len() {
        16 => &aead::AES_128_GCM,
        32 => &aead::AES_256_GCM,
        _ => return None,
    };
    let mut sealed = [cipher_text.as_slice(), tag.as_slice()].concat();
    open(algorithm, &content_key, iv.as_slice().try_into().ok()?, AAD, &mut sealed)
}

fn open(
    algorithm: &'static aead::Algorithm,
    key: &[u8],
    nonce: [u8; 12],
    aad: &[u8],
    sealed: &mut [u8],
) -> Option<Vec<u8>> {
    let key = LessSafeKey::new(UnboundKey::new(algorithm, key).ok()?);
    let nonce = Nonce::assume_unique_for_key(nonce);
    let plain = key.open_in_place(nonce, Aad::from(aad), sealed).ok()?;
    Some(plain.to_vec())
}

/// `Plug.Crypto.encrypt` stores the term with its signing time and maximum
/// age, in one of three layouts over plug_crypto's history. The web app reads
/// its records with `max_age: :infinity`, so the age is not checked here.
fn unwrap_signed(term: Term) -> Option<Term> {
    match term {
        Term::Tuple(items) if matches!(items.len(), 2 | 3) => items.into_iter().next(),
        map @ Term::Map(_) => map.get("data").cloned(),
        _ => None,
    }
}

/// HChaCha20 (draft-irtf-cfrg-xchacha, section 2.2), which derives the
/// ChaCha20 key for an XChaCha20 nonce. ring has ChaCha20-Poly1305 but not
/// this step.
fn hchacha20(key: &[u8; 32], nonce: &[u8; 16]) -> [u8; 32] {
    let mut state = [0u32; 16];
    // "expand 32-byte k"
    state[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]);
    for (word, bytes) in state[4..].iter_mut().zip(key.chunks_exact(4).chain(nonce.chunks_exact(4))) {
        *word = u32::from_le_bytes(bytes.try_into().expect("4 bytes"));
    }
    for _ in 0..10 {
        for [a, b, c, d] in [
            [0, 4, 8, 12],
            [1, 5, 9, 13],
            [2, 6, 10, 14],
            [3, 7, 11, 15],
            [0, 5, 10, 15],
            [1, 6, 11, 12],
            [2, 7, 8, 13],
            [3, 4, 9, 14],
        ] {
            state[a] = state[a].wrapping_add(state[b]);
            state[d] = (state[d] ^ state[a]).rotate_left(16);
            state[c] = state[c].wrapping_add(state[d]);
            state[b] = (state[b] ^ state[c]).rotate_left(12);
            state[a] = state[a].wrapping_add(state[b]);
            state[d] = (state[d] ^ state[a]).rotate_left(8);
            state[c] = state[c].wrapping_add(state[d]);
            state[b] = (state[b] ^ state[c]).rotate_left(7);
        }
    }
    let mut subkey = [0; 32];
    for (bytes, word) in subkey.chunks_exact_mut(4).zip(state[..4].iter().chain(&state[12..])) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    subkey
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn hchacha20_matches_the_draft_vector() {
        let key: [u8; 32] = std::array::from_fn(|i| i as u8);
        let nonce: [u8; 16] = hex("000000090000004a0000000031415927").try_into().unwrap();
        assert_eq!(
            hchacha20(&key, &nonce).to_vec(),
            hex("82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc")
        );
    }
}
