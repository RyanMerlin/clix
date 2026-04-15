//! Ed25519 pack signing utilities.
//!
//! Key storage:
//!   Private key: `~/.clix/pack-signing.pem` (PEM, mode 0600)
//!   Public key:  `~/.clix/pack-signing.pub` (PEM, mode 0644)
//!   Trusted keys: `~/.clix/trusted-pack-keys/<fingerprint>.pub`

use std::path::{Path, PathBuf};
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use rand_core::OsRng;
use sha2::{Sha256, Digest};
use crate::error::{ClixError, Result};

// ─── Fingerprint ─────────────────────────────────────────────────────────────

/// Compute the fingerprint of a verifying key: hex(SHA-256(pubkey_bytes))[..16].
pub fn key_fingerprint(vk: &VerifyingKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(vk.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

// ─── Key generation ───────────────────────────────────────────────────────────

/// Generate a new Ed25519 key pair and write to the given paths.
/// `private_path` is written as PEM PRIVATE KEY (pkcs8).
/// `public_path` is written as PEM PUBLIC KEY.
pub fn generate_keypair(private_path: &Path, public_path: &Path, force: bool) -> Result<String> {
    if private_path.exists() && !force {
        return Err(ClixError::Pack(format!(
            "key already exists at {}; use --force to overwrite",
            private_path.display()
        )));
    }

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    // Encode as PEM using ed25519_dalek's built-in pkcs8 support
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use ed25519_dalek::pkcs8::spki::EncodePublicKey;
    use pkcs8::LineEnding;

    let private_pem = signing_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| ClixError::Pack(format!("encode private key: {e}")))?;
    let public_pem = verifying_key
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| ClixError::Pack(format!("encode public key: {e}")))?;

    if let Some(parent) = private_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = public_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(private_path, private_pem.as_bytes())?;
    std::fs::write(public_path, public_pem.as_bytes())?;

    // Restrict private key permissions on Linux
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(private_path, std::fs::Permissions::from_mode(0o600));
        let _ = std::fs::set_permissions(public_path, std::fs::Permissions::from_mode(0o644));
    }

    Ok(key_fingerprint(&verifying_key))
}

// ─── Sign ─────────────────────────────────────────────────────────────────────

/// Load a signing key from a PEM private key file and sign the provided bytes.
/// Returns the Ed25519 signature bytes (64 bytes).
pub fn sign_bytes(private_key_path: &Path, data: &[u8]) -> Result<Signature> {
    let pem = std::fs::read_to_string(private_key_path)?;
    use ed25519_dalek::pkcs8::DecodePrivateKey;
    let signing_key = SigningKey::from_pkcs8_pem(&pem)
        .map_err(|e| ClixError::Pack(format!("parse signing key: {e}")))?;
    Ok(signing_key.sign(data))
}

/// Return the verifying key for a given private key PEM file.
pub fn verifying_key_from_private(private_key_path: &Path) -> Result<VerifyingKey> {
    let pem = std::fs::read_to_string(private_key_path)?;
    use ed25519_dalek::pkcs8::DecodePrivateKey;
    let signing_key = SigningKey::from_pkcs8_pem(&pem)
        .map_err(|e| ClixError::Pack(format!("parse signing key: {e}")))?;
    Ok(signing_key.verifying_key())
}

// ─── Verify ───────────────────────────────────────────────────────────────────

/// Load a verifying key from a PEM public key file.
pub fn load_verifying_key(public_key_path: &Path) -> Result<VerifyingKey> {
    let pem = std::fs::read_to_string(public_key_path)?;
    use ed25519_dalek::pkcs8::spki::DecodePublicKey;
    VerifyingKey::from_public_key_pem(&pem)
        .map_err(|e| ClixError::Pack(format!("parse public key: {e}")))
}

/// Verify a signature against data using the public keys in trusted_keys_dir.
/// Returns the fingerprint of the matching key on success.
pub fn verify_signature(
    data: &[u8],
    sig_bytes: &[u8; 64],
    trusted_keys_dir: &Path,
) -> Result<String> {
    let signature = Signature::from_bytes(sig_bytes);

    if !trusted_keys_dir.exists() {
        return Err(ClixError::Pack(
            "no trusted keys directory found — run `clix pack trust <pubkey>` first".to_string(),
        ));
    }

    let mut tried = 0usize;
    for entry in std::fs::read_dir(trusted_keys_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pub") {
            continue;
        }
        let vk = match load_verifying_key(&path) {
            Ok(k) => k,
            Err(_) => continue,
        };
        tried += 1;
        if vk.verify(data, &signature).is_ok() {
            return Ok(key_fingerprint(&vk));
        }
    }

    if tried == 0 {
        Err(ClixError::Pack(
            "no trusted public keys found — run `clix pack trust <pubkey>` first".to_string(),
        ))
    } else {
        Err(ClixError::Pack(
            "signature verification failed — pack is not signed by a trusted key".to_string(),
        ))
    }
}

// ─── Trust store ─────────────────────────────────────────────────────────────

/// Copy a public key into the trusted-pack-keys directory. Returns the fingerprint.
pub fn trust_key(pubkey_path: &Path, trusted_keys_dir: &Path) -> Result<String> {
    let vk = load_verifying_key(pubkey_path)?;
    let fp = key_fingerprint(&vk);

    std::fs::create_dir_all(trusted_keys_dir)?;
    let dest = trusted_keys_dir.join(format!("{fp}.pub"));

    // Read original PEM
    let pem = std::fs::read_to_string(pubkey_path)?;
    std::fs::write(&dest, pem)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o644));
    }

    Ok(fp)
}

// ─── Default paths ────────────────────────────────────────────────────────────

pub fn default_signing_key_path(home: &Path) -> PathBuf {
    home.join("pack-signing.pem")
}

pub fn default_public_key_path(home: &Path) -> PathBuf {
    home.join("pack-signing.pub")
}

pub fn default_trusted_keys_dir(home: &Path) -> PathBuf {
    home.join("trusted-pack-keys")
}
