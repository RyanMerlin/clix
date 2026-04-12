package clix

import "fmt"

var (
	Version   = "dev"
	Commit    = "none"
	BuildDate = "unknown"
)

func VersionInfo() map[string]any {
	return map[string]any{
		"version":   Version,
		"commit":    Commit,
		"buildDate": BuildDate,
	}
}

func VersionString() string {
	return fmt.Sprintf("%s (%s, %s)", Version, Commit, BuildDate)
}
