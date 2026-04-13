package clix

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"
)

// execCommand and execLookPath are thin wrappers so sandbox_test.go can swap them out.
var execCommand = exec.Command
var execLookPath = exec.LookPath

func Run() error {
	root := &cobra.Command{
		Use:     "clix",
		Short:   "clix is a governed CLI gateway",
		Version: VersionString(),
	}
	root.AddCommand(newInitCmd(), newCapabilitiesCmd(), newProfileCmd(), newPackCmd(), newWorkflowCmd(), newRunCmd(), newPolicyCmd(), newReceiptsCmd(), newDoctorCmd(), newServeCmd(), newClientCmd(), newApprovalCmd(), newSandboxCmd(), newVersionCmd())
	return root.Execute()
}

func loadOrSeed() (*State, error) {
	home := HomeDir()
	if _, err := os.Stat(home); os.IsNotExist(err) {
		return SeedState(home)
	}
	return LoadState(home)
}

func newInitCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "init",
		Short: "Seed local clix state",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := SeedState(HomeDir())
			if err != nil {
				return err
			}
			return printJSON(map[string]any{"ok": true, "home": state.Home})
		},
	}
}

func newCapabilitiesCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "capabilities"}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List known capabilities",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			registry, err := buildRegistry(state)
			if err != nil {
				return err
			}
			return printJSON(registry.All())
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "describe <name>",
		Short: "Describe a capability",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			registry, err := buildRegistry(state)
			if err != nil {
				return err
			}
			cap, ok := registry.Get(args[0])
			if !ok {
				return fmt.Errorf("unknown capability: %s", args[0])
			}
			return printJSON(cap)
		},
	})
	return cmd
}

func newProfileCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "profile"}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List installed profiles",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			profiles, err := LoadProfiles(state.ProfilesDir)
			if err != nil {
				return err
			}
			return printJSON(profiles)
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "describe <name>",
		Short: "Describe a profile",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			profiles, err := LoadProfiles(state.ProfilesDir)
			if err != nil {
				return err
			}
			for _, p := range profiles {
				if p.Name == args[0] {
					return printJSON(p)
				}
			}
			return fmt.Errorf("unknown profile: %s", args[0])
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "active",
		Short: "Show active profiles",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			return printJSON(state.Config.ActiveProfiles)
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "use <profile[,profile...]>",
		Short: "Set active profiles",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			next := splitCSV(args[0])
			state.Config.ActiveProfiles = next
			if len(state.Config.ActiveProfiles) == 0 {
				state.Config.ActiveProfiles = []string{"base"}
			}
			if err := writeJSON(state.ConfigPath, state.Config); err != nil {
				return err
			}
			return printJSON(map[string]any{"ok": true, "activeProfiles": state.Config.ActiveProfiles})
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "merge --profile a --profile b",
		Short: "Resolve a merged profile set",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			registry, err := buildRegistry(state)
			if err != nil {
				return err
			}
			names, _ := cmd.Flags().GetStringSlice("profile")
			merged, err := ResolveProfiles(state, names, registry)
			if err != nil {
				return err
			}
			return printJSON(merged)
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "inspect",
		Short: "Inspect merged active profiles",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			registry, err := buildRegistry(state)
			if err != nil {
				return err
			}
			merged, err := ResolveProfiles(state, state.ActiveProfiles(), registry)
			if err != nil {
				return err
			}
			return printJSON(merged)
		},
	})
	cmd.PersistentFlags().StringSlice("profile", nil, "profile names")
	return cmd
}

func newPackCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "pack"}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List installed packs",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			packs, err := loadPackManifests(state.PacksDir)
			if err != nil {
				return err
			}
			return printJSON(packs)
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "discover <path>",
		Short: "Inspect a pack at a source path",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			manifest, err := discoverPack(args[0])
			if err != nil {
				return err
			}
			return printJSON(manifest)
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "show <name>",
		Short: "Show an installed pack",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			packs, err := loadPackManifests(state.PacksDir)
			if err != nil {
				return err
			}
			for _, pack := range packs {
				if pack.Name == args[0] {
					return printJSON(pack)
				}
			}
			return fmt.Errorf("unknown pack: %s", args[0])
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "install <path>",
		Short: "Install a pack from a local directory or bundle",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			manifest, err := installPack(args[0], state.PacksDir, boolFlag(cmd, "force"))
			if err != nil {
				return err
			}
			return printJSON(map[string]any{"ok": true, "pack": manifest})
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "bundle <path>",
		Short: "Create a distributable pack bundle",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			target, _ := cmd.Flags().GetString("out")
			bundle, path, err := bundlePack(args[0], target)
			if err != nil {
				return err
			}
			sum, err := hashFile(path)
			if err != nil {
				return err
			}
			return printJSON(map[string]any{
				"ok":         true,
				"bundle":     bundle,
				"archive":    path,
				"sha256":     sum,
				"sha256File": path + ".sha256",
			})
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "publish <path>",
		Short: "Publish a pack bundle to a local registry directory",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			outDir, _ := cmd.Flags().GetString("to")
			force := boolFlag(cmd, "force")
			returnValue, err := publishPack(args[0], outDir, force)
			if err != nil {
				return err
			}
			return printJSON(returnValue)
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "scaffold <name>",
		Short: "Create a new pack scaffold",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			targetDir, _ := cmd.Flags().GetString("dir")
			if targetDir == "" {
				targetDir = filepath.Join(".", args[0])
			}
			manifest, err := scaffoldPackWithPreset(targetDir, args[0], mustString(cmd, "description"), mustString(cmd, "preset"), mustString(cmd, "command"), boolFlag(cmd, "force"))
			if err != nil {
				return err
			}
			return printJSON(map[string]any{"ok": true, "pack": manifest, "path": targetDir})
		},
	})
	onboardCmd := &cobra.Command{
		Use:   "onboard <name>",
		Short: "Probe a CLI and generate a first-pass pack",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			command := mustString(cmd, "command")
			if command == "" {
				return fmt.Errorf("command is required")
			}
			targetDir, _ := cmd.Flags().GetString("dir")
			if targetDir == "" {
				targetDir = filepath.Join(".", args[0])
			}
			manifest, report, err := onboardPack(targetDir, args[0], mustString(cmd, "description"), command, mustString(cmd, "runner"), mustString(cmd, "image"), boolFlag(cmd, "force"))
			if err != nil {
				return err
			}
			return printJSON(map[string]any{"ok": true, "pack": manifest, "onboard": report, "path": targetDir})
		},
	}
	cmd.AddCommand(onboardCmd)
	cmd.PersistentFlags().Bool("force", false, "overwrite an existing pack")
	cmd.PersistentFlags().String("dir", "", "target directory for the scaffold")
	cmd.PersistentFlags().String("description", "", "pack description")
	cmd.PersistentFlags().String("preset", "read-only", "scaffold preset: read-only, change-controlled, or operator")
	cmd.PersistentFlags().String("command", "", "external CLI command to bind to the scaffold")
	cmd.PersistentFlags().String("runner", "auto", "onboard probe runner: local, docker, podman, or auto")
	cmd.PersistentFlags().String("image", "", "container image for onboard probes")
	cmd.PersistentFlags().String("out", "", "bundle output path")
	cmd.PersistentFlags().String("to", "", "publish destination directory")
	return cmd
}

func newWorkflowCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "workflow"}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List workflows",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			workflows, err := loadWorkflows(state.WorkflowsDir)
			if err != nil {
				return err
			}
			return printJSON(workflows)
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "run <name>",
		Short: "Run a workflow",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			registry, err := buildRegistry(state)
			if err != nil {
				return err
			}
			workflows, err := buildWorkflowRegistry(state)
			if err != nil {
				return err
			}
			var input map[string]any
			if raw, _ := cmd.Flags().GetString("input"); raw != "" {
				if err := json.Unmarshal([]byte(raw), &input); err != nil {
					return err
				}
			}
			if input == nil {
				input = map[string]any{}
			}
			policy := state.Policy
			outcome, err := runWorkflow(state, registry, workflows, policy, args[0], input, ctxFromState(state), approvalMode(cmd))
			if err != nil {
				return err
			}
			return printJSON(outcome)
		},
	})
	cmd.Flags().String("input", "", "workflow input JSON")
	cmd.Flags().Bool("yes", false, "auto approve")
	return cmd
}

func newRunCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "run <capability>",
		Short: "Run a capability",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			registry, err := buildRegistry(state)
			if err != nil {
				return err
			}
			var input map[string]any
			if raw, _ := cmd.Flags().GetString("input"); raw != "" {
				if err := json.Unmarshal([]byte(raw), &input); err != nil {
					return err
				}
			}
			if input == nil {
				input = map[string]any{}
			}
			outcome, err := runCapability(state, registry, state.Policy, args[0], input, ctxFromState(state), approvalMode(cmd))
			if err != nil {
				return err
			}
			return printJSON(outcome)
		},
	}
	cmd.Flags().String("input", "", "capability input JSON")
	cmd.Flags().Bool("yes", false, "auto approve")
	return cmd
}

func newPolicyCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "policy"}
	cmd.AddCommand(&cobra.Command{
		Use:   "test <capability>",
		Short: "Evaluate policy for a capability",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			registry, err := buildRegistry(state)
			if err != nil {
				return err
			}
			cap, ok := registry.Get(args[0])
			if !ok {
				return fmt.Errorf("unknown capability: %s", args[0])
			}
			ctx := ctxFromState(state)
			return printJSON(EvaluatePolicy(state.Policy, ctx, cap))
		},
	})
	return cmd
}

func newReceiptsCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "receipts"}
	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List receipts",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			receipts, err := readReceipts(state.ReceiptsDir)
			if err != nil {
				return err
			}
			return printJSON(receipts)
		},
	})
	cmd.AddCommand(&cobra.Command{
		Use:   "show <id>",
		Short: "Show a receipt",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			receipt, err := findReceipt(state.ReceiptsDir, args[0])
			if err != nil {
				return err
			}
			if receipt == nil {
				return fmt.Errorf("receipt not found: %s", args[0])
			}
			return printJSON(receipt)
		},
	})
	return cmd
}

func newDoctorCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "doctor",
		Short: "Check local state",
		RunE: func(cmd *cobra.Command, args []string) error {
			home := HomeDir()
			checks := []map[string]any{
				{"name": "home", "ok": fileExists(home), "path": home},
				{"name": "config", "ok": fileExists(filepath.Join(home, "config.json")), "path": filepath.Join(home, "config.json")},
				{"name": "policy", "ok": fileExists(filepath.Join(home, "policy.json")), "path": filepath.Join(home, "policy.json")},
				{"name": "packs", "ok": fileExists(filepath.Join(home, "packs")), "path": filepath.Join(home, "packs")},
				{"name": "profiles", "ok": fileExists(filepath.Join(home, "profiles")), "path": filepath.Join(home, "profiles")},
			}
			return printJSON(map[string]any{"ok": true, "checks": checks})
		},
	}
}

func newVersionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Show build information",
		RunE: func(cmd *cobra.Command, args []string) error {
			return printJSON(VersionInfo())
		},
	}
}

// runUnsandboxed runs the agent without any Landlock restriction.
// Used when Landlock is unavailable and --require-landlock is not set.
func runUnsandboxed(argv0 string, args []string) error {
	c := execCommand(argv0, args[1:]...)
	c.Stdin = os.Stdin
	c.Stdout = os.Stdout
	c.Stderr = os.Stderr
	return c.Run()
}

func newSandboxCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "sandbox",
		Short: "Run a process inside a Landlock exec sandbox",
		Long: `Sandbox wraps a process with a Linux Landlock exec restriction.
The sandboxed process (and any child it spawns) can only exec binaries listed
in the sandbox.execAllowlist config plus the clix binary itself.

Requires Linux kernel 5.13+ (Landlock ABI v1).`,
	}

	// clix sandbox status — report Landlock availability.
	cmd.AddCommand(&cobra.Command{
		Use:   "status",
		Short: "Report Landlock availability on this kernel",
		RunE: func(cmd *cobra.Command, args []string) error {
			abi := LandlockStatus()
			available := abi >= 1
			return printJSON(map[string]any{
				"available":  available,
				"abiVersion": abi,
				"minRequired": 1,
			})
		},
	})

	// clix sandbox run -- <cmd> [args...]
	runCmd := &cobra.Command{
		Use:   "run -- <command> [args...]",
		Short: "Run a command inside a Landlock exec sandbox",
		Long: `Launches <command> inside a Landlock sandbox that restricts exec to an
allowlist. The allowlist always includes the clix binary; additional paths
come from sandbox.execAllowlist in config.json or --allow flags.

Example:
  clix sandbox run -- python3 agent.py
  clix sandbox run --allow /usr/bin/node -- node agent.js`,
		Args: cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			extraAllowlist, _ := cmd.Flags().GetStringSlice("allow")
			requireLandlock := boolFlag(cmd, "require-landlock") || state.Config.Sandbox.RequireLandlock

			// Resolve the clix binary path — always in the allowlist.
			self, err := os.Executable()
			if err != nil {
				return fmt.Errorf("sandbox: cannot determine clix binary path: %w", err)
			}

			// Build the full allowlist: clix + config + --allow flags.
			allowlist := dedupStrings(append(
				append([]string{self}, state.Config.Sandbox.ExecAllowlist...),
				extraAllowlist...,
			))

			// Resolve the agent executable.
			agentExe, err := lookPath(args[0])
			if err != nil {
				return fmt.Errorf("sandbox: cannot find %q: %w", args[0], err)
			}
			// The agent executable itself must be in the allowlist so it can start.
			allowlist = dedupStrings(append(allowlist, agentExe))

			// Check Landlock availability before launching the subprocess.
			if LandlockStatus() < 1 {
				if requireLandlock {
					return fmt.Errorf("sandbox: Landlock unavailable on this kernel (requires Linux 5.13+); refusing to run without enforcement (--require-landlock is set)")
				}
				fmt.Fprintf(os.Stderr, "clix sandbox: WARNING — Landlock unavailable on this kernel; running %s WITHOUT exec restriction\n", args[0])
				// Run the command directly without sandboxing.
				return runUnsandboxed(agentExe, args)
			}

			// Re-exec via the hidden `_exec` subcommand so that Landlock is applied
			// in a fresh process before execing the agent.
			// We pass the allowlist and agent command via args to _exec.
			clixArgs := []string{self, "sandbox", "_exec"}
			for _, p := range allowlist {
				clixArgs = append(clixArgs, "--allow", p)
			}
			clixArgs = append(clixArgs, "--")
			clixArgs = append(clixArgs, agentExe)
			clixArgs = append(clixArgs, args[1:]...)

			execCmd := execCommand(self, clixArgs[1:]...)
			execCmd.Stdin = os.Stdin
			execCmd.Stdout = os.Stdout
			execCmd.Stderr = os.Stderr
			return execCmd.Run()
		},
	}
	runCmd.Flags().StringSlice("allow", nil, "additional executable path to allow (may be repeated)")
	runCmd.Flags().Bool("require-landlock", false, "fail if Landlock is unavailable instead of running unsandboxed")
	cmd.AddCommand(runCmd)

	// clix sandbox _exec — hidden internal subcommand used by `sandbox run`.
	// Applies Landlock to the current process and exec's the target command.
	execInternalCmd := &cobra.Command{
		Use:    "_exec",
		Hidden: true,
		Args:   cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			allowlist, _ := cmd.Flags().GetStringSlice("allow")

			// Resolve the target executable (first non-flag arg).
			argv0, err := lookPath(args[0])
			if err != nil {
				return fmt.Errorf("sandbox _exec: cannot find %q: %w", args[0], err)
			}
			argv := append([]string{argv0}, args[1:]...)

			// Apply Landlock and exec. SandboxExec calls LockOSThread internally.
			if err := SandboxExec(allowlist, argv0, argv, os.Environ()); err != nil {
				return fmt.Errorf("sandbox _exec: %w", err)
			}
			return nil // unreachable on success
		},
	}
	execInternalCmd.Flags().StringSlice("allow", nil, "allowed executable path")
	cmd.AddCommand(execInternalCmd)

	return cmd
}

// dedupStrings returns a slice with duplicates removed, preserving order.
func dedupStrings(in []string) []string {
	seen := make(map[string]bool, len(in))
	out := make([]string, 0, len(in))
	for _, s := range in {
		if s != "" && !seen[s] {
			seen[s] = true
			out = append(out, s)
		}
	}
	return out
}

// lookPath wraps exec.LookPath for use in cli.go.
func lookPath(name string) (string, error) {
	return execLookPath(name)
}

func newApprovalCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "approval",
		Short: "Manage the approval gate",
	}

	// clix approval test — sends a synthetic approval request to the configured webhook.
	cmd.AddCommand(&cobra.Command{
		Use:   "test",
		Short: "Send a test approval request to the configured webhook",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			if state.Config.ApprovalGate.WebhookURL == "" {
				return fmt.Errorf("no approvalGate.webhookUrl configured in config.json")
			}
			approved, approver, reason, err := requestApproval(
				state.Config.ApprovalGate,
				CapabilityManifest{Name: "test.approval-ping", Risk: "low", Description: "Synthetic test request from clix approval test"},
				map[string]any{"test": true},
				ctxFromState(state),
				map[string]any{"decision": "require_approval", "reason": "test ping"},
			)
			if err != nil {
				return err
			}
			return printJSON(map[string]any{
				"ok":       approved,
				"approved": approved,
				"approver": approver,
				"reason":   reason,
				"webhook":  state.Config.ApprovalGate.WebhookURL,
			})
		},
	})

	// clix approval show — display current approval gate config (no secrets).
	cmd.AddCommand(&cobra.Command{
		Use:   "show",
		Short: "Show the current approval gate configuration",
		RunE: func(cmd *cobra.Command, args []string) error {
			state, err := loadOrSeed()
			if err != nil {
				return err
			}
			cfg := state.Config.ApprovalGate
			configured := cfg.WebhookURL != ""
			timeout := cfg.TimeoutSeconds
			if timeout <= 0 {
				timeout = defaultApprovalTimeoutSeconds
			}
			return printJSON(map[string]any{
				"configured":     configured,
				"webhookUrl":     cfg.WebhookURL,
				"timeoutSeconds": timeout,
				"headers":        headerKeys(cfg.Headers),
			})
		},
	})

	return cmd
}

// headerKeys returns just the header key names (not values) for safe display.
func headerKeys(h map[string]string) []string {
	keys := make([]string, 0, len(h))
	for k := range h {
		keys = append(keys, k)
	}
	return keys
}

func newClientCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "client <capability>",
		Short: "Forward a capability call to a running clix daemon",
		Long: `Connects to a clix daemon over a Unix socket or HTTP and invokes a capability.
The daemon address is resolved in order:
  1. --socket flag (Unix socket path)
  2. CLIX_SOCKET environment variable
  3. --http flag (HTTP address)`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			socketFlag, _ := cmd.Flags().GetString("socket")
			httpFlag, _ := cmd.Flags().GetString("http")
			addr := daemonSocket(socketFlag)

			var input map[string]any
			if raw, _ := cmd.Flags().GetString("input"); raw != "" {
				if err := json.Unmarshal([]byte(raw), &input); err != nil {
					return fmt.Errorf("invalid --input JSON: %w", err)
				}
			}
			if input == nil {
				input = map[string]any{}
			}
			params := map[string]any{
				"name":      args[0],
				"arguments": input,
			}

			var (
				result map[string]any
				err    error
			)
			switch {
			case addr != "":
				path := strings.TrimPrefix(addr, "unix://")
				result, err = callDaemonSocket(path, "tools/call", params)
			case httpFlag != "":
				result, err = callDaemonHTTP(httpFlag, "tools/call", params)
			default:
				return fmt.Errorf("no daemon address: set --socket, CLIX_SOCKET, or --http")
			}
			if err != nil {
				return err
			}
			return printJSON(result)
		},
	}
	cmd.Flags().String("socket", "", "Unix socket path of the clix daemon")
	cmd.Flags().String("http", "", "HTTP address of the clix daemon (e.g. localhost:8080)")
	cmd.Flags().String("input", "", "capability input JSON")
	return cmd
}

func printJSON(v any) error {
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}

func splitCSV(s string) []string {
	parts := strings.Split(s, ",")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

func boolFlag(cmd *cobra.Command, name string) bool {
	v, _ := cmd.Flags().GetBool(name)
	return v
}

func approvalMode(cmd *cobra.Command) string {
	if boolFlag(cmd, "yes") {
		return "auto"
	}
	return "interactive"
}

func ctxFromState(state *State) map[string]string {
	return map[string]string{
		"env":     state.Config.DefaultEnv,
		"cwd":     state.Config.WorkspaceRoot,
		"user":    currentUserName(),
		"profile": strings.Join(state.ActiveProfiles(), ","),
	}
}

func currentUserName() string {
	if v := os.Getenv("USER"); v != "" {
		return v
	}
	if v := os.Getenv("USERNAME"); v != "" {
		return v
	}
	return "unknown"
}

func mustString(cmd *cobra.Command, name string) string {
	v, _ := cmd.Flags().GetString(name)
	return v
}
