#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod stub;

pub fn apply_sandbox(allowed_executables: &[String]) -> crate::error::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::apply_sandbox(allowed_executables);
    #[cfg(not(target_os = "linux"))]
    return stub::apply_sandbox(allowed_executables);
}

pub fn sandbox_enforced() -> bool {
    #[cfg(target_os = "linux")]
    return linux::sandbox_enforced();
    #[cfg(not(target_os = "linux"))]
    return stub::sandbox_enforced();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_sandbox_flag() { let _ = sandbox_enforced(); }
    #[test]
    fn test_empty_allowlist_noop() { apply_sandbox(&[]).unwrap(); }
}
