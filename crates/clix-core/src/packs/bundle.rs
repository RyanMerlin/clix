use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use crate::error::{ClixError, Result};
use super::install::copy_dir_all;
use crate::manifest::loader::load_manifest;
use crate::manifest::pack::PackManifest;

/// Bundle a pack directory into a .clixpack.zip archive with a .sha256 sidecar.
/// Returns the path to the created zip.
pub fn bundle_pack(pack_path: &Path, out_dir: &Path) -> Result<PathBuf> {
    let manifest_path = ["pack.yaml", "pack.yml", "pack.json"]
        .iter()
        .map(|f| pack_path.join(f))
        .find(|p| p.exists())
        .ok_or_else(|| ClixError::Pack("pack.yaml not found".to_string()))?;
    let manifest: PackManifest = load_manifest(&manifest_path)?;

    std::fs::create_dir_all(out_dir)?;
    let zip_name = format!("{}-v{}.clixpack.zip", manifest.name, manifest.version);
    let zip_path = out_dir.join(&zip_name);

    let file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, pack_path, pack_path, &options)?;
    zip.finish().map_err(|e| ClixError::Pack(format!("zip finish: {e}")))?;

    let data = std::fs::read(&zip_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let checksum = hex::encode(hasher.finalize());
    let sha_path = zip_path.with_extension("clixpack.sha256");
    std::fs::write(&sha_path, format!("{checksum}  {zip_name}\n"))?;

    Ok(zip_path)
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    base: &Path,
    current: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.strip_prefix(base).unwrap().to_str().unwrap().replace('\\', "/");
        if path.is_dir() {
            zip.add_directory(&name, *options)
                .map_err(|e| ClixError::Pack(format!("zip dir: {e}")))?;
            add_dir_to_zip(zip, base, &path, options)?;
        } else {
            zip.start_file(&name, *options)
                .map_err(|e| ClixError::Pack(format!("zip file: {e}")))?;
            let mut f = std::fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}

/// Copy a bundle archive to bundles_dir/published/ and update index.json.
pub fn publish_pack(zip_path: &Path, bundles_dir: &Path) -> Result<()> {
    let published = bundles_dir.join("published");
    std::fs::create_dir_all(&published)?;
    let dest = published.join(zip_path.file_name().unwrap());
    std::fs::copy(zip_path, &dest)?;

    let index_path = published.join("index.json");
    let mut index: Vec<String> = if index_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&index_path)?).unwrap_or_default()
    } else {
        vec![]
    };
    let entry = zip_path.file_name().unwrap().to_string_lossy().to_string();
    if !index.contains(&entry) {
        index.push(entry);
    }
    std::fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;
    Ok(())
}

// suppress unused import warning for copy_dir_all
#[allow(unused_imports)]
use copy_dir_all as _;
