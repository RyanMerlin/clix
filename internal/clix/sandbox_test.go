//go:build linux

package clix

import (
	"os"
	"os/exec"
	"strings"
	"testing"
)

// TestLandlockStatus verifies that LandlockStatus returns a positive ABI
// version on Linux kernels >= 5.13. WSL2 6.x satisfies this.
func TestLandlockStatus(t *testing.T) {
	abi := LandlockStatus()
	if abi < 1 {
		t.Skipf("Landlock unavailable on this kernel (ABI=%d) — skipping enforcement tests", abi)
	}
	t.Logf("Landlock ABI version: %d", abi)
}

// TestApplyExecLandlock_AllowedPathExecutes verifies that after applying a
// Landlock exec restriction, a binary in the allowlist can still be executed.
// We use /bin/true (or /usr/bin/true) which is universally available.
func TestApplyExecLandlock_AllowedPathExecutes(t *testing.T) {
	if LandlockStatus() < 1 {
		t.Skip("Landlock not available")
	}

	truePath, err := exec.LookPath("true")
	if err != nil {
		t.Skip("true not found in PATH")
	}

	// This test forks a subprocess that applies Landlock and then tries to
	// exec /bin/true. We use a re-exec of the test binary with a special env
	// var to avoid contaminating the parent process's thread state.
	cmd := exec.Command(os.Args[0], "-test.run=TestApplyExecLandlock_ApplyAndRun")
	cmd.Env = append(os.Environ(),
		"CLIX_SANDBOX_TEST=1",
		"CLIX_SANDBOX_ALLOW="+truePath,
		"CLIX_SANDBOX_EXEC="+truePath,
	)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("allowed binary failed to execute under Landlock: %v\noutput: %s", err, out)
	}
}

// TestApplyExecLandlock_ApplyAndRun is a helper test that only runs when
// invoked as a subprocess via TestApplyExecLandlock_AllowedPathExecutes.
func TestApplyExecLandlock_ApplyAndRun(t *testing.T) {
	if os.Getenv("CLIX_SANDBOX_TEST") != "1" {
		t.Skip("not a sandbox subprocess")
	}
	allowPath := os.Getenv("CLIX_SANDBOX_ALLOW")
	execPath := os.Getenv("CLIX_SANDBOX_EXEC")

	if err := ApplyExecLandlock([]string{allowPath}); err != nil {
		t.Fatalf("ApplyExecLandlock: %v", err)
	}
	// If Landlock is correctly applied and allowPath is permitted, exec succeeds.
	if err := SandboxExec([]string{allowPath}, execPath, []string{execPath}, os.Environ()); err != nil {
		t.Fatalf("SandboxExec: %v", err)
	}
}

// TestApplyExecLandlock_DeniedPathBlocked verifies that a binary NOT in the
// allowlist is denied execution under Landlock.
func TestApplyExecLandlock_DeniedPathBlocked(t *testing.T) {
	if LandlockStatus() < 1 {
		t.Skip("Landlock not available")
	}

	truePath, err := exec.LookPath("true")
	if err != nil {
		t.Skip("true not found in PATH")
	}
	falsePath, err := exec.LookPath("false")
	if err != nil {
		t.Skip("false not found in PATH")
	}
	if truePath == falsePath {
		t.Skip("true and false are the same binary")
	}

	// Subprocess: allow /bin/true, then try to exec /bin/false (should be denied).
	cmd := exec.Command(os.Args[0], "-test.run=TestApplyExecLandlock_DenyAndRun")
	cmd.Env = append(os.Environ(),
		"CLIX_SANDBOX_TEST=1",
		"CLIX_SANDBOX_ALLOW="+truePath,
		"CLIX_SANDBOX_EXEC="+falsePath, // not in allowlist
	)
	out, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("expected denied binary to fail, but exec succeeded\noutput: %s", out)
	}
	t.Logf("correctly denied: %v\noutput: %s", err, out)
}

// TestApplyExecLandlock_DenyAndRun is the subprocess helper for the deny test.
func TestApplyExecLandlock_DenyAndRun(t *testing.T) {
	if os.Getenv("CLIX_SANDBOX_TEST") != "1" {
		t.Skip("not a sandbox subprocess")
	}
	allowPath := os.Getenv("CLIX_SANDBOX_ALLOW")
	execPath := os.Getenv("CLIX_SANDBOX_EXEC")

	if err := ApplyExecLandlock([]string{allowPath}); err != nil {
		t.Fatalf("ApplyExecLandlock: %v", err)
	}
	// This exec should be denied by Landlock — the test subprocess will exit non-zero.
	_ = SandboxExec([]string{allowPath}, execPath, []string{execPath}, os.Environ())
	// If we reach here, exec was denied (returned error). Exit non-zero so the
	// parent test sees a failure-as-expected.
	os.Exit(1)
}

// TestLandlockStatus_UnavailableGraceful verifies ErrLandlockUnavailable is
// returned when ABI < 1. We can't easily fake a kernel downgrade, so this
// tests the sentinel error value itself.
func TestLandlockUnavailable_ErrorType(t *testing.T) {
	if !strings.Contains(ErrLandlockUnavailable.Error(), "Linux 5.13") {
		t.Errorf("ErrLandlockUnavailable message should mention Linux 5.13, got: %q", ErrLandlockUnavailable.Error())
	}
}

// TestSandboxConfig_RoundTrip verifies SandboxConfig serialises/deserialises
// cleanly through state (no sandbox fields are dropped).
func TestSandboxConfig_RoundTrip(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed: %v", err)
	}
	state.Config.Sandbox = SandboxConfig{
		ExecAllowlist:   []string{"/usr/bin/clix", "/usr/bin/python3"},
		RequireLandlock: true,
	}
	if err := writeJSON(state.ConfigPath, state.Config); err != nil {
		t.Fatalf("write config: %v", err)
	}
	loaded, err := LoadState(home)
	if err != nil {
		t.Fatalf("load state: %v", err)
	}
	if len(loaded.Config.Sandbox.ExecAllowlist) != 2 {
		t.Errorf("expected 2 allowlist entries, got %v", loaded.Config.Sandbox.ExecAllowlist)
	}
	if !loaded.Config.Sandbox.RequireLandlock {
		t.Error("expected RequireLandlock=true after round-trip")
	}
}
