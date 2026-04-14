use std::path::Path;
use crate::error::Result;
use super::install::copy_dir_all;

/// Seed the built-in packs from the embedded packs directory into packs_dir.
/// Built-in packs are already installed if the directory exists.
pub fn seed_builtin_packs(packs_dir: &Path, builtin_packs_src: &Path) -> Result<()> {
    if !builtin_packs_src.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(builtin_packs_src)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let pack_name = entry.file_name();
        let dest = packs_dir.join(&pack_name);
        if !dest.exists() {
            copy_dir_all(&entry.path(), &dest)?;
        }
    }
    Ok(())
}
