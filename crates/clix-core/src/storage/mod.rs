use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Filesystem abstraction. All persistent reads/writes in clix-core should go
/// through this trait so tests can substitute an in-memory backend and future
/// code can target git or S3 without touching business logic.
pub trait Storage: Send + Sync {
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn mkdir_p(&self, path: &Path) -> io::Result<()>;
    /// List immediate children of `path` as absolute `PathBuf`s.
    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn copy_dir(&self, src: &Path, dst: &Path) -> io::Result<()>;
}

// ── FsStorage ─────────────────────────────────────────────────────────────────

/// Thin wrapper around `std::fs`. This is the default storage backend.
#[derive(Clone, Debug)]
pub struct FsStorage;

impl Storage for FsStorage {
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    fn mkdir_p(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)? {
            entries.push(entry?.path());
        }
        Ok(entries)
    }

    fn copy_dir(&self, src: &Path, dst: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                self.copy_dir(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }
}

// ── MemStorage ────────────────────────────────────────────────────────────────

/// In-memory storage backend for tests. NOT thread-safe for concurrent writes —
/// wrap in a Mutex if needed.
#[cfg(test)]
pub mod mem {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct MemStorage {
        pub files: Mutex<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MemStorage {
        pub fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        pub fn seed(&self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) {
            self.files.lock().unwrap().insert(path.into(), content.into());
        }
    }

    impl Storage for MemStorage {
        fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.files.lock().unwrap().get(path).cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
        }

        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            let bytes = self.read_bytes(path)?;
            String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        }

        fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
            self.files.lock().unwrap().insert(path.to_path_buf(), bytes.to_vec());
            Ok(())
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap().contains_key(path)
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.files.lock().unwrap().remove(path);
            Ok(())
        }

        fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
            let mut files = self.files.lock().unwrap();
            let prefix = path.to_path_buf();
            files.retain(|k, _| !k.starts_with(&prefix));
            Ok(())
        }

        fn mkdir_p(&self, _path: &Path) -> io::Result<()> {
            Ok(()) // directories are implicit in the in-memory store
        }

        fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
            let files = self.files.lock().unwrap();
            let mut children: Vec<PathBuf> = files
                .keys()
                .filter_map(|k| {
                    let rel = k.strip_prefix(path).ok()?;
                    let component = rel.components().next()?;
                    Some(path.join(component))
                })
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            children.sort();
            Ok(children)
        }

        fn copy_dir(&self, src: &Path, dst: &Path) -> io::Result<()> {
            let mut files = self.files.lock().unwrap();
            let copies: Vec<(PathBuf, Vec<u8>)> = files
                .iter()
                .filter_map(|(k, v)| {
                    let rel = k.strip_prefix(src).ok()?;
                    Some((dst.join(rel), v.clone()))
                })
                .collect();
            for (dst_path, content) in copies {
                files.insert(dst_path, content);
            }
            Ok(())
        }
    }
}

/// Convenience alias for the shared storage handle used throughout clix-core.
pub type StorageRef = Arc<dyn Storage>;

/// Construct the default filesystem-backed storage handle.
pub fn default_storage() -> StorageRef {
    Arc::new(FsStorage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::mem::MemStorage;
    use std::path::Path;

    #[test]
    fn mem_storage_round_trip() {
        let store = MemStorage::new();
        let p = Path::new("/a/b/c.txt");
        assert!(!store.exists(p));
        store.write(p, b"hello").unwrap();
        assert!(store.exists(p));
        assert_eq!(store.read_to_string(p).unwrap(), "hello");
        assert_eq!(store.read_bytes(p).unwrap(), b"hello");
    }

    #[test]
    fn mem_storage_remove() {
        let store = MemStorage::new();
        let p = Path::new("/x/y.txt");
        store.write(p, b"data").unwrap();
        store.remove_file(p).unwrap();
        assert!(!store.exists(p));
    }

    #[test]
    fn mem_storage_remove_dir_all() {
        let store = MemStorage::new();
        store.write(Path::new("/root/a.txt"), b"a").unwrap();
        store.write(Path::new("/root/sub/b.txt"), b"b").unwrap();
        store.write(Path::new("/other/c.txt"), b"c").unwrap();
        store.remove_dir_all(Path::new("/root")).unwrap();
        assert!(!store.exists(Path::new("/root/a.txt")));
        assert!(!store.exists(Path::new("/root/sub/b.txt")));
        assert!(store.exists(Path::new("/other/c.txt")));
    }

    #[test]
    fn mem_storage_list() {
        let store = MemStorage::new();
        store.write(Path::new("/d/a.txt"), b"").unwrap();
        store.write(Path::new("/d/b.txt"), b"").unwrap();
        store.write(Path::new("/d/sub/c.txt"), b"").unwrap();
        let mut children = store.list(Path::new("/d")).unwrap();
        children.sort();
        assert_eq!(children, vec![
            PathBuf::from("/d/a.txt"),
            PathBuf::from("/d/b.txt"),
            PathBuf::from("/d/sub"),
        ]);
    }

    #[test]
    fn mem_storage_copy_dir() {
        let store = MemStorage::new();
        store.write(Path::new("/src/x.txt"), b"x").unwrap();
        store.write(Path::new("/src/nested/y.txt"), b"y").unwrap();
        store.copy_dir(Path::new("/src"), Path::new("/dst")).unwrap();
        assert_eq!(store.read_to_string(Path::new("/dst/x.txt")).unwrap(), "x");
        assert_eq!(store.read_to_string(Path::new("/dst/nested/y.txt")).unwrap(), "y");
    }

    #[test]
    fn clix_state_load_with_storage() {
        use crate::state::ClixState;
        use std::sync::Arc;

        let store = MemStorage::new();
        let config_yaml = "schemaVersion: 1\ndefaultEnv: test\n";
        store.write(Path::new("/home/.clix/config.yaml"), config_yaml.as_bytes()).unwrap();

        let state = ClixState::load_with_storage(
            PathBuf::from("/home/.clix"),
            Arc::clone(&store) as StorageRef,
        ).unwrap();
        assert_eq!(state.config.default_env, "test");
    }
}
