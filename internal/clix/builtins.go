package clix

import (
	"os"
	"os/exec"
	"runtime"
)

func builtinHandler(name string, input map[string]any) (map[string]any, error) {
	switch name {
	case "system.date":
		return map[string]any{"iso": currentISO()}, nil
	case "shell.echo":
		return map[string]any{"echoed": true, "message": toString(input["message"])}, nil
	case "node.version":
		return map[string]any{"version": runtime.Version(), "executable": os.Args[0]}, nil
	default:
		return nil, exec.ErrNotFound
	}
}
