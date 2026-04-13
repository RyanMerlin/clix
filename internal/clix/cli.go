package clix

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"
)

func Run() error {
	root := &cobra.Command{
		Use:     "clix",
		Short:   "clix is a governed CLI gateway",
		Version: VersionString(),
	}
	root.AddCommand(newInitCmd(), newCapabilitiesCmd(), newProfileCmd(), newPackCmd(), newWorkflowCmd(), newRunCmd(), newPolicyCmd(), newReceiptsCmd(), newDoctorCmd(), newServeCmd(), newClientCmd(), newVersionCmd())
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
