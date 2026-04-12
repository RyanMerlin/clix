package clix

import (
	"bytes"
	"context"
	"fmt"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

type OnboardProbe struct {
	Label    string   `json:"label"`
	Runner   string   `json:"runner,omitempty"`
	Image    string   `json:"image,omitempty"`
	Command  string   `json:"command"`
	Args     []string `json:"args"`
	ExitCode int      `json:"exitCode"`
	Output   string   `json:"output,omitempty"`
	Error    string   `json:"error,omitempty"`
}

type OnboardReport struct {
	Name             string         `json:"name"`
	Command          string         `json:"command"`
	Runner           string         `json:"runner,omitempty"`
	Image            string         `json:"image,omitempty"`
	Preset           string         `json:"preset"`
	ObservedCommands []string       `json:"observedCommands,omitempty"`
	Probes           []OnboardProbe `json:"probes,omitempty"`
}

func onboardPack(targetDir, name, description, command, runner, image string, force bool) (PackManifest, OnboardReport, error) {
	if command == "" {
		return PackManifest{}, OnboardReport{}, fmt.Errorf("command is required")
	}
	probes, err := probeCLI(command, runner, image)
	if err != nil {
		return PackManifest{}, OnboardReport{}, err
	}
	aggregate := aggregateProbeText(probes)
	commands := extractObservedCommands(aggregate)
	preset := inferPackPreset(aggregate, commands)
	if description == "" {
		description = fmt.Sprintf("Generated from onboarding probes for %s.", command)
	}
	manifest, err := scaffoldPackWithPreset(targetDir, name, description, preset, command, force)
	if err != nil {
		return PackManifest{}, OnboardReport{}, err
	}
	report := OnboardReport{
		Name:             name,
		Command:          command,
		Runner:           probes[0].Runner,
		Image:            image,
		Preset:           preset,
		ObservedCommands: commands,
		Probes:           probes,
	}
	if err := writeJSON(filepath.Join(targetDir, "onboard.json"), report); err != nil {
		return PackManifest{}, OnboardReport{}, err
	}
	return manifest, report, nil
}

func probeCLI(command, runner, image string) ([]OnboardProbe, error) {
	resolvedRunner, err := resolveProbeRunner(runner, image)
	if err != nil {
		return nil, err
	}
	specs := []struct {
		label string
		args  []string
	}{
		{label: "help-flag", args: []string{"--help"}},
		{label: "help-command", args: []string{"help"}},
		{label: "version-flag", args: []string{"--version"}},
		{label: "version-command", args: []string{"version"}},
		{label: "info", args: []string{"info"}},
		{label: "info-help", args: []string{"info", "--help"}},
	}
	probes := make([]OnboardProbe, 0, len(specs))
	for _, spec := range specs {
		probe := OnboardProbe{
			Label:   spec.label,
			Runner:  resolvedRunner,
			Image:   image,
			Command: command,
			Args:    spec.args,
		}
		output, exitCode, err := runProbeCommand(resolvedRunner, command, image, spec.args...)
		probe.ExitCode = exitCode
		probe.Output = output
		if err != nil {
			probe.Error = err.Error()
		}
		probes = append(probes, probe)
	}
	if aggregateProbeText(probes) == "" {
		return probes, fmt.Errorf("unable to collect probe output for %s", command)
	}
	return probes, nil
}

func resolveProbeRunner(runner, image string) (string, error) {
	if image == "" {
		if runner == "" || runner == "auto" || runner == "local" {
			return "local", nil
		}
		if runner == "docker" || runner == "podman" {
			return "", fmt.Errorf("%s requires --image", runner)
		}
		return "", fmt.Errorf("unknown runner: %s", runner)
	}
	switch runner {
	case "", "auto":
		if path, err := exec.LookPath("docker"); err == nil && path != "" {
			return path, nil
		}
		if path, err := exec.LookPath("podman"); err == nil && path != "" {
			return path, nil
		}
		return "", fmt.Errorf("no container runtime found; install docker or podman, or omit --image")
	case "docker", "podman":
		if path, err := exec.LookPath(runner); err == nil && path != "" {
			return path, nil
		}
		return "", fmt.Errorf("%s is not installed", runner)
	case "local":
		return "", fmt.Errorf("container image requires runner auto, docker, or podman")
	default:
		if path, err := exec.LookPath(runner); err == nil && path != "" {
			return path, nil
		}
		return "", fmt.Errorf("unknown runner: %s", runner)
	}
}

func runProbeCommand(runtime, command, image string, args ...string) (string, int, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()

	var cmd *exec.Cmd
	switch runtime {
	case "local":
		cmd = exec.CommandContext(ctx, command, args...)
	default:
		containerArgs := append([]string{"run", "--rm"}, image, command)
		containerArgs = append(containerArgs, args...)
		cmd = exec.CommandContext(ctx, runtime, containerArgs...)
	}
	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	output := strings.TrimSpace(stdout.String())
	if stderr.Len() > 0 {
		if output != "" {
			output += "\n"
		}
		output += strings.TrimSpace(stderr.String())
	}
	exitCode := 0
	if err != nil {
		exitCode = 1
		if exitErr, ok := err.(*exec.ExitError); ok {
			exitCode = exitErr.ExitCode()
		}
		if output == "" {
			return "", exitCode, err
		}
		return output, exitCode, nil
	}
	return output, exitCode, nil
}

func aggregateProbeText(probes []OnboardProbe) string {
	var parts []string
	for _, probe := range probes {
		if strings.TrimSpace(probe.Output) != "" {
			parts = append(parts, probe.Output)
		}
	}
	return strings.Join(parts, "\n")
}

func inferPackPreset(text string, commands []string) string {
	lower := strings.ToLower(text)
	if containsAny(lower, []string{"reconcile", "verify", "sync", "apply", "operator"}) && containsAny(lower, []string{"status", "health", "rollout"}) {
		return "operator"
	}
	if containsAny(lower, []string{"plan", "apply", "diff", "dry run", "dry-run", "preview"}) {
		return "change-controlled"
	}
	if preset := inferPackPresetFromCommands(commands); preset != "" {
		return preset
	}
	return "read-only"
}

func inferPackPresetFromCommands(commands []string) string {
	joined := strings.ToLower(strings.Join(commands, " "))
	switch {
	case containsAny(joined, []string{"reconcile", "verify", "status"}) && containsAny(joined, []string{"apply", "sync", "rollout"}):
		return "operator"
	case containsAny(joined, []string{"plan", "apply", "diff", "preview"}):
		return "change-controlled"
	case containsAny(joined, []string{"info", "version", "help", "list", "get", "show"}):
		return "read-only"
	default:
		return ""
	}
}

func containsAny(text string, needles []string) bool {
	for _, needle := range needles {
		if strings.Contains(text, needle) {
			return true
		}
	}
	return false
}

func extractObservedCommands(text string) []string {
	if strings.TrimSpace(text) == "" {
		return nil
	}
	lines := strings.Split(text, "\n")
	seen := map[string]struct{}{}
	var commands []string
	inCommands := false
	for _, raw := range lines {
		line := strings.TrimRight(raw, "\r")
		trimmed := strings.TrimSpace(line)
		lower := strings.ToLower(trimmed)
		switch {
		case strings.HasPrefix(lower, "commands:"),
			strings.HasPrefix(lower, "available commands:"),
			strings.HasPrefix(lower, "subcommands:"),
			strings.HasPrefix(lower, "command groups:"):
			inCommands = true
			continue
		case inCommands && trimmed == "":
			continue
		case inCommands && (strings.HasPrefix(lower, "flags:") || strings.HasPrefix(lower, "global flags:") || strings.HasPrefix(lower, "options:") || strings.HasPrefix(lower, "examples:")):
			inCommands = false
			continue
		}
		if !inCommands {
			continue
		}
		if !strings.HasPrefix(line, " ") && !strings.HasPrefix(line, "\t") {
			continue
		}
		fields := strings.Fields(trimmed)
		if len(fields) == 0 {
			continue
		}
		name := strings.Trim(fields[0], "`")
		if !isLikelyCommandName(name) {
			continue
		}
		if _, ok := seen[name]; ok {
			continue
		}
		seen[name] = struct{}{}
		commands = append(commands, name)
	}
	sort.Strings(commands)
	return commands
}

func isLikelyCommandName(name string) bool {
	if name == "" || strings.HasPrefix(name, "-") {
		return false
	}
	for _, r := range name {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r >= '0' && r <= '9':
		case r == '.' || r == '_' || r == '-' || r == ':' || r == '/':
		default:
			return false
		}
	}
	return true
}
