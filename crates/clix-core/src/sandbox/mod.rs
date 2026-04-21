#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod stub;

pub mod jail;

#[cfg(target_os = "linux")]
pub mod seccomp;

pub fn apply_sandbox(allowed_executables: &[impl AsRef<str>]) -> crate::error::Result<()> {
    let paths: Vec<String> = allowed_executables.iter().map(|s| s.as_ref().to_string()).collect();
    #[cfg(target_os = "linux")]
    return linux::apply_sandbox(&paths);
    #[cfg(target_os = "macos")]
    return macos::apply_sandbox(&paths);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return stub::apply_sandbox(&paths);
}

pub fn sandbox_enforced() -> bool {
    #[cfg(target_os = "linux")]
    return linux::sandbox_enforced();
    #[cfg(target_os = "macos")]
    return macos::sandbox_enforced();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return stub::sandbox_enforced();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_sandbox_flag() { let _ = sandbox_enforced(); }
    #[test]
    fn test_empty_allowlist_noop() { let empty: Vec<String> = vec![]; apply_sandbox(&empty).unwrap(); }
}
