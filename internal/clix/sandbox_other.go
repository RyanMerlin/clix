//go:build !linux

package clix

import "errors"

// ErrLandlockUnavailable is returned on non-Linux platforms.
var ErrLandlockUnavailable = errors.New("landlock is only available on Linux 5.13+")

// LandlockStatus returns 0 on non-Linux platforms.
func LandlockStatus() int { return 0 }

// ApplyExecLandlock always returns ErrLandlockUnavailable on non-Linux platforms.
func ApplyExecLandlock(allowedPaths []string) error { return ErrLandlockUnavailable }

// SandboxExec always returns ErrLandlockUnavailable on non-Linux platforms.
func SandboxExec(allowedPaths []string, argv0 string, argv []string, env []string) error {
	return ErrLandlockUnavailable
}
