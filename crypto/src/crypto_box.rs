// Copyright (c) SimpleStaking, Viable Systems and Tezedge Contributors
// SPDX-License-Identifier: MIT

//! This module wrapps [`sodiumoxide::crypto::box_::`] stuff,
//! which is used for encrypt/decrypt messages between peers.
//!
//! Terminology:
//!
//! PublicKey - [`CRYPTO_KEY_SIZE`]-bytes
//! SecretKey - [`CRYPTO_KEY_SIZE`]-bytes
//! PrecomputedKey - [`CRYPTO_KEY_SIZE`]-bytes created from PublicKey and SecretKey
//!
//! CryptoboxPublicKeyHash - generated as a hash of [`PublicKey`], for example used as a peer_id

#[cfg(all(feature = "std", not(feature = "no_sodium")))]
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt::{self, Debug};

use hex::{FromHex, FromHexError};
#[cfg(all(feature = "std", not(feature = "no_sodium")))]
use sodiumoxide::crypto::box_;

#[cfg(feature = "no_sodium")]
use cryptoxide::x25519;

use crate::{blake2b::Blake2bError, hash::FromBytesError, CryptoError};

use super::hash::CryptoboxPublicKeyHash;

#[cfg(feature = "rand")]
use super::nonce::Nonce;

use thiserror::Error;

pub const BOX_ZERO_BYTES: usize = 32;
pub const CRYPTO_KEY_SIZE: usize = 32;

pub trait CryptoKey: Sized {
    fn from_bytes<B: AsRef<[u8]>>(buf: B) -> Result<Self, CryptoError>;
}

#[derive(Debug, Error, PartialEq)]
pub enum PublicKeyError {
    #[error("Error constructing hash: {0}")]
    HashError(#[from] FromBytesError),
    #[error("Blake2b digest error: {0}")]
    Blake2bError(#[from] Blake2bError),
}

fn ensure_crypto_key_bytes<B: AsRef<[u8]>>(buf: B) -> Result<[u8; CRYPTO_KEY_SIZE], CryptoError> {
    let buf = buf.as_ref();

    // check size
    if buf.len() != CRYPTO_KEY_SIZE {
        return Err(CryptoError::InvalidKeySize {
            expected: CRYPTO_KEY_SIZE,
            actual: buf.len(),
        });
    };

    // convert to correct key size
    let mut arr = [0u8; CRYPTO_KEY_SIZE];
    arr.copy_from_slice(buf);
    Ok(arr)
}

/// Convenience wrapper around [`sodiumoxide::crypto::box_::PublicKey`]
#[cfg(all(feature = "std", not(feature = "no_sodium")))]
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct PublicKey(box_::PublicKey);
#[cfg(feature = "no_sodium")]
pub struct PublicKey(x25519::PublicKey);

impl PublicKey {
    /// Generates public key hash for public key
    pub fn public_key_hash(&self) -> Result<CryptoboxPublicKeyHash, PublicKeyError> {
        CryptoboxPublicKeyHash::try_from(crate::blake2b::digest_128(self.0.as_ref())?)
            .map_err(PublicKeyError::from)
    }
}

impl CryptoKey for PublicKey {
    fn from_bytes<B: AsRef<[u8]>>(buf: B) -> Result<Self, CryptoError> {
        ensure_crypto_key_bytes(buf).map(|key_bytes| PublicKey(x25519::PublicKey::from(key_bytes)))
    }
}

#[cfg(all(feature = "std", not(feature = "no_sodium")))]
impl AsRef<box_::PublicKey> for PublicKey {
    fn as_ref(&self) -> &box_::PublicKey {
        &self.0
    }
}
#[cfg(feature = "no_sodium")]
impl AsRef<x25519::PublicKey> for PublicKey {
    fn as_ref(&self) -> &x25519::PublicKey {
        &self.0
    }
}

impl FromHex for PublicKey {
    type Error = CryptoError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        Self::from_bytes(hex::decode(hex)?)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
/// Convenience wrapper around [`sodiumoxide::crypto::box_::SecretKey`]
#[cfg(all(feature = "std", not(feature = "no_sodium")))]
pub struct SecretKey(box_::SecretKey);
#[cfg(feature = "no_sodium")]
pub struct SecretKey(x25519::SecretKey);

impl CryptoKey for SecretKey {
    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    fn from_bytes<B: AsRef<[u8]>>(buf: B) -> Result<Self, CryptoError> {
        ensure_crypto_key_bytes(buf).map(|key_bytes| SecretKey(box_::SecretKey(key_bytes)))
    }
    #[cfg(feature = "no_sodium")]
    fn from_bytes<B: AsRef<[u8]>>(buf: B) -> Result<Self, CryptoError> {
        ensure_crypto_key_bytes(buf).map(|key_bytes| SecretKey(x25519::SecretKey::from(key_bytes)))
    }
}

#[cfg(all(feature = "std", not(feature = "no_sodium")))]
impl AsRef<box_::SecretKey> for SecretKey {
    fn as_ref(&self) -> &box_::SecretKey {
        &self.0
    }
}
#[cfg(feature = "no_sodium")]
impl AsRef<x25519::SecretKey> for SecretKey {
    fn as_ref(&self) -> &x25519::SecretKey {
        &self.0
    }
}

impl FromHex for SecretKey {
    type Error = CryptoError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        Self::from_bytes(hex::decode(hex)?)
    }
}

/// Generates random keypair: [`PublicKey, SecretKey`] + [`CryptoboxPublicKeyHash`]
///
/// Note: Strange why it is called pair, bud returns triplet :)
#[cfg(all(feature = "std", not(feature = "no_sodium")))]
pub fn random_keypair() -> Result<(SecretKey, PublicKey, CryptoboxPublicKeyHash), PublicKeyError> {
    // generate
    let (pk, sk) = box_::gen_keypair();

    // wrap it
    let sk = SecretKey(sk);
    let pk = PublicKey(pk);

    // generate public key hash
    let pkh = pk.public_key_hash()?;

    // return
    Ok((sk, pk, pkh))
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone)]
/// Convenience wrapper around [`sodiumoxide::crypto::box_::PrecomputedKey`]
#[cfg(all(feature = "std", not(feature = "no_sodium")))]
pub struct PrecomputedKey(box_::PrecomputedKey);
#[cfg(feature = "no_sodium")]
pub struct PrecomputedKey(x25519::SharedSecret);

#[cfg(feature = "fuzzing")]
impl fuzzcheck::DefaultMutator for PrecomputedKey {
    type Mutator = fuzzcheck::mutators::unit::UnitMutator<PrecomputedKey>;
    #[no_coverage]
    fn default_mutator() -> Self::Mutator {
        let pk = PrecomputedKey::from_bytes([0; box_::PRECOMPUTEDKEYBYTES]);
        fuzzcheck::mutators::unit::UnitMutator::new(pk)
    }
}

impl PrecomputedKey {
    /// Create `PrecomputedKey` from public key and secret key
    ///
    /// # Arguments
    /// * `pk_as_hex_string` - Hex string representing public key
    /// * `sk_as_hex_string` - Hex string representing secret key
    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    pub fn precompute(pk: &PublicKey, sk: &SecretKey) -> Self {
        Self(box_::precompute(pk.as_ref(), sk.as_ref()))
    }
    #[cfg(feature = "no_sodium")]
    pub fn precompute(pk: &PublicKey, sk: &SecretKey) -> Self {
        Self(x25519::dh(sk.as_ref(), pk.as_ref()))
    }

    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    pub fn from_bytes(bytes: [u8; box_::PRECOMPUTEDKEYBYTES]) -> Self {
        Self(box_::PrecomputedKey(bytes))
    }
    #[cfg(feature = "no_sodium")]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(x25519::SharedSecret::from(bytes))
    }

    /// Encrypt binary message
    ///
    /// # Arguments
    /// * `msg` - Binary message to be encoded
    /// * `nonce` - Nonce required to encode message
    /// * `pck` - Precomputed key required to encode message
    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    pub fn encrypt(&self, msg: &[u8], nonce: &Nonce) -> Result<Vec<u8>, CryptoError> {
        let box_nonce = box_::Nonce(nonce.get_bytes()?);
        Ok(box_::seal_precomputed(msg, &box_nonce, &self.0))
    }

    /// Decrypt binary message into raw binary data
    ///
    /// # Arguments
    /// * `enc` - Encoded message
    /// * `nonce` - Nonce required to decode message
    /// * `pck` - Precomputed key required to decode message
    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    pub fn decrypt(&self, enc: &[u8], nonce: &Nonce) -> Result<Vec<u8>, CryptoError> {
        let box_nonce = box_::Nonce(nonce.get_bytes()?);

        match box_::open_precomputed(enc, &box_nonce, &self.0) {
            Ok(msg) => Ok(msg),
            Err(()) => Err(CryptoError::FailedToDecrypt),
        }
    }
}

impl Debug for PrecomputedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrecomputedKey(****)")
    }
}

impl From<FromHexError> for CryptoError {
    fn from(e: FromHexError) -> Self {
        CryptoError::InvalidKey {
            reason: format!("{}", e),
        }
    }
}

#[cfg(test)]
mod tests {

    use std::convert::TryInto;

    use super::*;
    use crate::nonce::NONCE_SIZE;

    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    #[test]
    fn generate_nonce_xsalsa20() {
        let nonce = box_::gen_nonce();
        assert_eq!(NONCE_SIZE, nonce.0.len())
    }

    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    #[test]
    fn generate_precomputed_key() -> Result<(), anyhow::Error> {
        let pk = PublicKey::from_hex(
            "96678b88756dd6cfd6c129980247b70a6e44da77823c3672a2ec0eae870d8646",
        )?;
        let sk = SecretKey::from_hex(
            "a18dc11cb480ebd31081e1541df8bd70c57da0fa419b5036242f8619d605e75a",
        )?;
        let pck = PrecomputedKey::precompute(&pk, &sk);

        let precomputed = hex::encode(&pck.0);
        let expected_precomputed =
            "5228751a6f5a6494e38e1042f578e3a64ae3462b7899356f49e50be846c9609c";
        assert_eq!(expected_precomputed, precomputed);

        Ok(())
    }

    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    #[test]
    fn encrypt_message() -> Result<(), anyhow::Error> {
        let pk = PublicKey::from_hex(
            "96678b88756dd6cfd6c129980247b70a6e44da77823c3672a2ec0eae870d8646",
        )?;
        let sk = SecretKey::from_hex(
            "a18dc11cb480ebd31081e1541df8bd70c57da0fa419b5036242f8619d605e75a",
        )?;
        let pck = PrecomputedKey::precompute(&pk, &sk);

        let nonce = Nonce::new(
            &hex::decode("8dde158c55cff52f4be9352787d333e616a67853640d72c5")?
                .try_into()
                .unwrap(),
        );
        let msg = hex::decode("00874d1b98317bd6efad8352a7144c9eb0b218c9130e0a875973908ddc894b764ffc0d7f176cf800b978af9e919bdc35122585168475096d0ebcaca1f2a1172412b91b363ff484d1c64c03417e0e755e696c386a0000002d53414e44424f5845445f54455a4f535f414c5048414e45545f323031382d31312d33305431353a33303a35365a00000000")?;

        let encrypted_msg = pck.encrypt(&msg, &nonce)?;
        let expected_encrypted_msg = hex::decode("45d82d5c4067f5c32748596c1bbc93a9f87b5b1f2058ddd82b6f081ca484b672395c7473ab897c64c01c33878ac1ccb6919a75c9938d8bcf0e7917ddac13a787cfb5c9a5aea50d24502cf86b5c9b000358c039334ec077afe98936feec0dabfff35f14cafd2cd3173bbd56a7c6e5bf6f5f57c92b59b129918a5895e883e7d999b191aad078c4a5b164144c1beaed58b49ba9be094abf3a3bd9")?;
        assert_eq!(expected_encrypted_msg, encrypted_msg);

        Ok(())
    }

    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    #[test]
    fn decrypt_message() -> Result<(), anyhow::Error> {
        let pk = PublicKey::from_hex(
            "96678b88756dd6cfd6c129980247b70a6e44da77823c3672a2ec0eae870d8646",
        )?;
        let sk = SecretKey::from_hex(
            "a18dc11cb480ebd31081e1541df8bd70c57da0fa419b5036242f8619d605e75a",
        )?;
        let pck = PrecomputedKey::precompute(&pk, &sk);

        let nonce = Nonce::new(
            &hex::decode("8dde158c55cff52f4be9352787d333e616a67853640d72c5")?
                .try_into()
                .unwrap(),
        );
        let enc = hex::decode("45d82d5c4067f5c32748596c1bbc93a9f87b5b1f2058ddd82b6f081ca484b672395c7473ab897c64c01c33878ac1ccb6919a75c9938d8bcf0e7917ddac13a787cfb5c9a5aea50d24502cf86b5c9b000358c039334ec077afe98936feec0dabfff35f14cafd2cd3173bbd56a7c6e5bf6f5f57c92b59b129918a5895e883e7d999b191aad078c4a5b164144c1beaed58b49ba9be094abf3a3bd9")?;

        let decrypted_msg = pck.decrypt(&enc, &nonce)?;
        let expected_decrypted_msg = hex::decode("00874d1b98317bd6efad8352a7144c9eb0b218c9130e0a875973908ddc894b764ffc0d7f176cf800b978af9e919bdc35122585168475096d0ebcaca1f2a1172412b91b363ff484d1c64c03417e0e755e696c386a0000002d53414e44424f5845445f54455a4f535f414c5048414e45545f323031382d31312d33305431353a33303a35365a00000000")?;
        assert_eq!(expected_decrypted_msg, decrypted_msg);

        Ok(())
    }

    #[cfg(all(feature = "std", not(feature = "no_sodium")))]
    #[test]
    fn decryption_of_encrypted_should_equal_message() -> Result<(), anyhow::Error> {
        let pk = PublicKey::from_hex(
            "96678b88756dd6cfd6c129980247b70a6e44da77823c3672a2ec0eae870d8646",
        )?;
        let sk = SecretKey::from_hex(
            "a18dc11cb480ebd31081e1541df8bd70c57da0fa419b5036242f8619d605e75a",
        )?;
        let pck = PrecomputedKey::precompute(&pk, &sk);

        let nonce = Nonce::new(
            &hex::decode("8dde158c55cff52f4be9352787d333e616a67853640d72c5")?
                .try_into()
                .unwrap(),
        );
        let msg = "hello world";

        let enc = pck.encrypt(msg.as_bytes(), &nonce)?;
        let dec = String::from_utf8(pck.decrypt(&enc, &nonce).unwrap())?;
        assert_eq!(msg, &dec);

        Ok(())
    }
}
