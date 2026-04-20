/// Verify the sandbox_enforced() contract on each platform.
use clix_core::sandbox::sandbox_enforced;

#[cfg(not(target_os = "linux"))]
#[test]
fn non_linux_sandbox_not_enforced() {
    assert!(
        !sandbox_enforced(),
        "sandbox_enforced() must return false on non-Linux; \
         capabilities run without OS isolation and receipts carry sandbox_enforced=false"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_sandbox_enforced_is_true() {
    assert!(
        sandbox_enforced(),
        "sandbox_enforced() should return true on Linux (the jail is available)"
    );
}
