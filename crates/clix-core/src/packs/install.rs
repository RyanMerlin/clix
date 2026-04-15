use std::path::{Path, PathBuf};
use crate::error::{ClixError, Result};
use crate::manifest::loader::load_manifest;
use crate::manifest::pack::PackManifest;
use super::signing;

/// Install a pack from a source directory or .clixpack.zip archive into packs_dir.
/// Returns the installed pack directory path.
pub fn install_pack(src: &Path, packs_dir: &Path) -> Result<PathBuf> {
    install_pack_verified(src, packs_dir, false, None)
}

pub fn install_pack_verified(
    src: &Path,
    packs_dir: &Path,
    verify_sig: bool,
    trusted_keys_dir: Option<&Path>,
) -> Result<PathBuf> {
    if src.is_file() {
        install_from_zip(src, packs_dir, verify_sig, trusted_keys_dir)
    } else if src.is_dir() {
        install_from_dir(src, packs_dir)
    } else {
        Err(ClixError::Pack(format!("pack source not found: {}", src.display())))
    }
}


fn install_from_dir(src: &Path, packs_dir: &Path) -> Result<PathBuf> {
    let manifest_path = {
        let yaml = src.join("pack.yaml");
        if yaml.exists() { yaml } else { src.join("pack.json") }
    };
    let manifest: PackManifest = load_manifest(&manifest_path)?;
    let dest = packs_dir.join(&manifest.name);
    copy_dir_all(src, &dest)?;
    Ok(dest)
}

fn install_from_zip(
    zip_path: &Path,
    packs_dir: &Path,
    verify_sig: bool,
    trusted_keys_dir: Option<&Path>,
) -> Result<PathBuf> {
    let sha_path = zip_path.with_extension("clixpack.sha256");
    if sha_path.exists() {
        verify_checksum(zip_path, &sha_path)?;
    }

    // Signature verification (after checksum passes)
    if verify_sig {
        let sig_path = PathBuf::from(format!("{}.sig", zip_path.display()));
        if !sig_path.exists() {
            return Err(ClixError::Pack("no signature found — pack is unsigned".to_string()));
        }
        let sig_hex = std::fs::read_to_string(&sig_path)?;
        let sig_hex = sig_hex.trim();
        let sig_bytes = hex::decode(sig_hex)
            .map_err(|e| ClixError::Pack(format!("decode signature hex: {e}")))?;
        if sig_bytes.len() != 64 {
            return Err(ClixError::Pack(format!(
                "signature length wrong: expected 64 bytes, got {}",
                sig_bytes.len()
            )));
        }
        let sig_arr: [u8; 64] = sig_bytes.try_into()
            .map_err(|_| ClixError::Pack("signature must be 64 bytes".to_string()))?;

        // Re-compute SHA-256 of the zip for verification
        use sha2::{Sha256, Digest};
        let data = std::fs::read(zip_path)?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let sha256_bytes: [u8; 32] = hasher.finalize().into();

        let fallback_dir;
        let keys_dir = match trusted_keys_dir {
            Some(d) => d,
            None => {
                fallback_dir = crate::packs::signing::default_trusted_keys_dir(
                    &crate::state::home_dir(),
                );
                &fallback_dir
            }
        };

        signing::verify_signature(&sha256_bytes, &sig_arr, keys_dir)?;
    }

    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ClixError::Pack(format!("zip open: {e}")))?;

    let pack_name = read_pack_name_from_zip(&mut archive)?;
    let dest = packs_dir.join(&pack_name);
    std::fs::create_dir_all(&dest)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| ClixError::Pack(format!("zip entry: {e}")))?;
        let out_path = dest.join(file.name());
        // Guard against zip slip: reject paths that escape the destination directory
        if !out_path.starts_with(&dest) {
            return Err(ClixError::Pack(format!(
                "unsafe path in archive: {}",
                file.name()
            )));
        }
        if file.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut file, &mut out)?;
        }
    }
    Ok(dest)
}

fn read_pack_name_from_zip(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<String> {
    for name in ["pack.yaml", "pack.yml", "pack.json"] {
        if let Ok(mut f) = archive.by_name(name) {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut f, &mut buf)?;
            let manifest: PackManifest = if name.ends_with(".json") {
                serde_json::from_str(&buf)?
            } else {
                serde_yaml::from_str(&buf)?
            };
            return Ok(manifest.name);
        }
    }
    Err(ClixError::Pack("pack.yaml not found in archive".to_string()))
}

fn verify_checksum(zip_path: &Path, sha_path: &Path) -> Result<()> {
    use sha2::{Sha256, Digest};
    let data = std::fs::read(zip_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual = hex::encode(hasher.finalize());
    let expected = std::fs::read_to_string(sha_path)?.trim().to_string();
    let expected_hash = expected.split_whitespace().next().unwrap_or(&expected);
    if actual != expected_hash {
        return Err(ClixError::Pack(format!(
            "checksum mismatch: expected {expected_hash}, got {actual}"
        )));
    }
    Ok(())
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_install_from_directory() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("pack.yaml"), "name: test-pack\nversion: 1\n").unwrap();
        let dest = TempDir::new().unwrap();
        install_pack(src.path(), dest.path()).unwrap();
        assert!(dest.path().join("test-pack").join("pack.yaml").exists());
    }
}
