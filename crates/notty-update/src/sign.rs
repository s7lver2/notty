//! Verificación Ed25519 de la firma de `notty-setup.exe` (verify-only en el build
//! por defecto; firmar es cosa de `notty-sign`, detrás de la feature `sign`).

use ed25519_dalek::{Signature, VerifyingKey};

pub fn verify(bytes: &[u8], sig: &[u8; 64], pubkey: &[u8; 32]) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(pubkey) else { return false };
    let signature = Signature::from_bytes(sig);
    vk.verify_strict(bytes, &signature).is_ok()
}

#[cfg(all(test, feature = "sign"))]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    #[test]
    fn valid_signature_verifies() {
        let sk = SigningKey::generate(&mut OsRng);
        let pubkey = sk.verifying_key().to_bytes();
        let msg = b"notty-setup.exe bytes go here";
        let sig = sk.sign(msg).to_bytes();
        assert!(verify(msg, &sig, &pubkey));
    }

    #[test]
    fn altered_bytes_fail() {
        let sk = SigningKey::generate(&mut OsRng);
        let pubkey = sk.verifying_key().to_bytes();
        let sig = sk.sign(b"original").to_bytes();
        assert!(!verify(b"altered!", &sig, &pubkey));
    }

    #[test]
    fn wrong_pubkey_fails() {
        let sk = SigningKey::generate(&mut OsRng);
        let other = SigningKey::generate(&mut OsRng);
        let msg = b"message";
        let sig = sk.sign(msg).to_bytes();
        assert!(!verify(msg, &sig, &other.verifying_key().to_bytes()));
    }
}
