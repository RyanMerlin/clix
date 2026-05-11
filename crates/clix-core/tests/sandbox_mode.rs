/// Verify the sandbox_enforced() contract on each platform.
use clix_core::sandbox::sandbox_enforced;

#[cfg(target_os = "linux")]
#[test]
fn linux_sandbox_enforced_is_true() {
    assert!(
        sandbox_enforced(),
        "sandbox_enforced() should return true on Linux (the jail is available)"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_sandbox_enforced_matches_sandbox_exec() {
    // On macOS, the sandbox is enforced when /usr/bin/sandbox-exec is present
    // (the standard system binary on all modern macOS). This test pins the
    // contract: sandbox_enforced() must agree with the presence check.
    let sandbox_exec_present = std::path::Path::new("/usr/bin/sandbox-exec").exists();
    assert_eq!(
        sandbox_enforced(),
        sandbox_exec_present,
        "sandbox_enforced() on macOS must equal sandbox-exec availability"
    );
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[test]
fn other_platforms_sandbox_not_enforced() {
    assert!(
        !sandbox_enforced(),
        "sandbox_enforced() must return false on platforms without an OS-level sandbox impl; \
         capabilities run without OS isolation and receipts carry sandbox_enforced=false"
    );
}
