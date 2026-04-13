//go:build linux

// Package clix — OS-level exec restriction via Linux Landlock.
//
// Landlock is a Linux Security Module (available since kernel 5.13) that lets
// unprivileged processes restrict their own filesystem access. clix uses it to
// apply an exec allowlist to the agent process: after ApplyExecLandlock(), the
// process (and any process it execs) can only execve() binaries whose paths are
// explicitly permitted.
//
// The restriction survives exec (inherited), so the pattern is:
//
//  1. clix sandbox run -- <agent>
//  2. clix launches itself as: clix sandbox _exec --allow <path>... -- <agent>
//  3. _exec: calls ApplyExecLandlock(allowedPaths)
//  4. _exec: syscall.Exec(agent, args, env)    — replaces process image
//  5. Agent now runs under Landlock — cannot exec kubectl, gcloud, etc.
//
// Graceful degradation: if the kernel does not support Landlock (< 5.13),
// ApplyExecLandlock returns ErrLandlockUnavailable. Callers decide whether to
// treat this as fatal (RequireLandlock) or warn-and-continue.
package clix

import (
	"debug/elf"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"unsafe"

	"golang.org/x/sys/unix"
)

// ErrLandlockUnavailable is returned when the kernel does not support Landlock
// or the ABI version is too old to enforce exec restrictions.
var ErrLandlockUnavailable = errors.New("landlock not available on this kernel (requires Linux 5.13+)")

// landlockRuleTypePath mirrors LANDLOCK_RULE_PATH_BENEATH from linux/landlock.h.
const landlockRuleTypePath = 1

// LandlockStatus probes the kernel for Landlock support and returns the ABI
// version (1 = Linux 5.13, 4 = Linux 6.7), or 0 if unavailable.
func LandlockStatus() int {
	// landlock_create_ruleset with LANDLOCK_CREATE_RULESET_VERSION (flags=1)
	// returns the ABI version when attr=NULL and size=0.
	abi, _, errno := unix.Syscall(unix.SYS_LANDLOCK_CREATE_RULESET, 0, 0, 1)
	if errno != 0 {
		return 0
	}
	return int(abi)
}

// elfInterpreter returns the PT_INTERP path from an ELF binary (e.g.
// /lib/x86_64-linux-gnu/ld-linux-x86-64.so.2), or "" if the file is not ELF,
// has no interpreter segment, or cannot be read.
func elfInterpreter(path string) string {
	f, err := elf.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	for _, prog := range f.Progs {
		if prog.Type == elf.PT_INTERP {
			// PT_INTERP data is a null-terminated path string.
			data := make([]byte, prog.Filesz)
			if _, err := prog.ReadAt(data, 0); err != nil {
				return ""
			}
			// Trim null terminator.
			if len(data) > 0 && data[len(data)-1] == 0 {
				data = data[:len(data)-1]
			}
			return string(data)
		}
	}
	return ""
}

// expandAllowlist augments paths with ELF interpreter paths so that Landlock
// FS_EXECUTE doesn't block the dynamic linker when loading allowed binaries.
func expandAllowlist(paths []string) []string {
	seen := make(map[string]bool, len(paths))
	result := make([]string, 0, len(paths)+2)
	add := func(p string) {
		if p != "" && !seen[p] {
			seen[p] = true
			result = append(result, p)
		}
	}
	for _, p := range paths {
		add(p)
	}
	// For each binary, also allow its ELF interpreter and the interpreter's dir.
	for _, p := range paths {
		if interp := elfInterpreter(p); interp != "" {
			add(interp)
			add(filepath.Dir(interp))
		}
	}
	return result
}

// ApplyExecLandlock restricts the calling OS thread (and any process it execs)
// to only execve() files within the listed paths.
//
// allowedPaths should contain the full paths to each binary or directory of
// binaries that should remain executable (e.g. os.Executable(), /usr/bin/python3).
//
// The caller must hold runtime.LockOSThread() before calling. SandboxExec does
// this automatically; direct callers are responsible.
//
// Once applied the restriction cannot be lifted.
func ApplyExecLandlock(allowedPaths []string) error {
	if LandlockStatus() < 1 {
		return ErrLandlockUnavailable
	}

	// Expand paths to include ELF interpreters so dynamic linking works.
	allowedPaths = expandAllowlist(allowedPaths)

	// Create a ruleset scoped to FS_EXECUTE access only.
	attr := unix.LandlockRulesetAttr{
		Access_fs: unix.LANDLOCK_ACCESS_FS_EXECUTE,
	}
	rulesetFD, _, errno := unix.Syscall(
		unix.SYS_LANDLOCK_CREATE_RULESET,
		uintptr(unsafe.Pointer(&attr)),
		unsafe.Sizeof(attr),
		0,
	)
	if errno != 0 {
		return fmt.Errorf("landlock: create ruleset: %w", errno)
	}
	defer unix.Close(int(rulesetFD))

	// Add an allow-execute rule for each permitted path.
	for _, path := range allowedPaths {
		if err := addExecAllowRule(int(rulesetFD), path); err != nil {
			// Skip paths that don't exist — they can't be exec'd anyway.
			if os.IsNotExist(err) {
				continue
			}
			return fmt.Errorf("landlock: allow %q: %w", path, err)
		}
	}

	// PR_SET_NO_NEW_PRIVS must be set before landlock_restrict_self.
	// It prevents privilege escalation via setuid binaries.
	if _, _, errno := unix.Syscall(unix.SYS_PRCTL, unix.PR_SET_NO_NEW_PRIVS, 1, 0); errno != 0 {
		return fmt.Errorf("landlock: set no_new_privs: %w", errno)
	}

	// Apply the ruleset to the calling OS thread.
	if _, _, errno := unix.Syscall(unix.SYS_LANDLOCK_RESTRICT_SELF, rulesetFD, 0, 0); errno != 0 {
		return fmt.Errorf("landlock: restrict self: %w", errno)
	}

	return nil
}

// addExecAllowRule opens path with O_PATH and adds a path-beneath rule
// allowing FS_EXECUTE. O_PATH is required by the Landlock API — it opens
// the path as a reference without triggering read-permission checks.
func addExecAllowRule(rulesetFD int, path string) error {
	fd, err := unix.Open(path, unix.O_PATH|unix.O_CLOEXEC, 0)
	if err != nil {
		return err
	}
	defer unix.Close(fd)

	rule := unix.LandlockPathBeneathAttr{
		Allowed_access: unix.LANDLOCK_ACCESS_FS_EXECUTE,
		Parent_fd:      int32(fd),
	}
	_, _, errno := unix.Syscall(
		unix.SYS_LANDLOCK_ADD_RULE,
		uintptr(rulesetFD),
		uintptr(landlockRuleTypePath),
		uintptr(unsafe.Pointer(&rule)),
	)
	if errno != 0 {
		return errno
	}
	return nil
}

// SandboxExec applies Landlock exec restrictions and then replaces the current
// process with the given command via unix.Exec. It never returns on success.
//
// This is the core of `clix sandbox _exec`. It locks the OS thread, applies
// Landlock on that thread, then exec's the target — the exec'd process inherits
// the Landlock domain from the calling thread.
func SandboxExec(allowedPaths []string, argv0 string, argv []string, env []string) error {
	// LockOSThread ensures Landlock and exec happen on the same OS thread.
	// Not deferred-unlocked: exec replaces the process and the lock is moot.
	runtime.LockOSThread()

	if err := ApplyExecLandlock(allowedPaths); err != nil {
		return err
	}

	// Replace the current process image with the target. Does not return on success.
	return unix.Exec(argv0, argv, env)
}
