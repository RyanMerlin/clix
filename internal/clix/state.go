package clix

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// InfisicalConfig holds connection and auth settings for the Infisical secrets backend.
// ClientID and ClientSecret are resolved at runtime: explicit values here take precedence,
// then INFISICAL_UNIVERSAL_AUTH_CLIENT_ID / INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET env vars.
type InfisicalConfig struct {
	SiteURL      string `json:"siteUrl,omitempty"`
	ClientID     string `json:"clientId,omitempty"`
	ClientSecret string `json:"clientSecret,omitempty"`
}

type Config struct {
	SchemaVersion  int             `json:"schemaVersion"`
	ApprovalMode   string          `json:"approvalMode"`
	DefaultEnv     string          `json:"defaultEnv"`
	WorkspaceRoot  string          `json:"workspaceRoot"`
	ActiveProfiles []string        `json:"activeProfiles"`
	Infisical      InfisicalConfig `json:"infisical,omitempty"`
}

type State struct {
	Home            string
	ConfigPath      string
	PolicyPath      string
	PacksDir        string
	ProfilesDir     string
	CapabilitiesDir string
	WorkflowsDir    string
	ReceiptsDir     string
	PluginsDir      string
	ApprovalsDir    string
	CacheDir        string
	Config          Config
	Policy          PolicyBundle
}

func HomeDir() string {
	if v := os.Getenv("CLIX_HOME"); v != "" {
		return v
	}
	dir, err := os.UserHomeDir()
	if err != nil {
		return ".clix"
	}
	return filepath.Join(dir, ".clix")
}

func Paths(home string) State {
	return State{
		Home:            home,
		ConfigPath:      filepath.Join(home, "config.json"),
		PolicyPath:      filepath.Join(home, "policy.json"),
		PacksDir:        filepath.Join(home, "packs"),
		ProfilesDir:     filepath.Join(home, "profiles"),
		CapabilitiesDir: filepath.Join(home, "capabilities"),
		WorkflowsDir:    filepath.Join(home, "workflows"),
		ReceiptsDir:     filepath.Join(home, "receipts"),
		PluginsDir:      filepath.Join(home, "plugins"),
		ApprovalsDir:    filepath.Join(home, "approvals"),
		CacheDir:        filepath.Join(home, "cache"),
	}
}

func ensureDir(dir string) error {
	return os.MkdirAll(dir, 0o755)
}

func readJSON(path string, out any) error {
	b, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	return json.Unmarshal(b, out)
}

func writeJSON(path string, in any) error {
	if err := ensureDir(filepath.Dir(path)); err != nil {
		return err
	}
	b, err := json.MarshalIndent(in, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o644)
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func LoadState(home string) (*State, error) {
	base := Paths(home)
	if err := readJSON(base.ConfigPath, &base.Config); err != nil {
		return nil, fmt.Errorf("load config: %w", err)
	}
	if err := readJSON(base.PolicyPath, &base.Policy); err != nil {
		return nil, fmt.Errorf("load policy: %w", err)
	}
	return &base, nil
}

func SeedState(home string) (*State, error) {
	base := Paths(home)
	for _, dir := range []string{
		base.Home, base.PacksDir, base.ProfilesDir, base.CapabilitiesDir, base.WorkflowsDir,
		base.ReceiptsDir, base.PluginsDir, base.ApprovalsDir, base.CacheDir,
	} {
		if err := ensureDir(dir); err != nil {
			return nil, err
		}
	}

	if !fileExists(base.ConfigPath) {
		cwd, _ := os.Getwd()
		cfg := Config{
			SchemaVersion:  1,
			ApprovalMode:   "interactive",
			DefaultEnv:     "local",
			WorkspaceRoot:  cwd,
			ActiveProfiles: []string{"base"},
		}
		if err := writeJSON(base.ConfigPath, cfg); err != nil {
			return nil, err
		}
	}

	if !fileExists(base.PolicyPath) {
		policy := PolicyBundle{
			SchemaVersion:   1,
			DefaultDecision: "deny",
			Rules: []PolicyRule{
				{Effect: "allow", Match: PolicyMatch{Profiles: []string{"base"}, SideEffects: []string{"read_only"}}},
				{Effect: "require_approval", Match: PolicyMatch{Risk: []string{"medium", "high"}}},
				{Effect: "deny", Match: PolicyMatch{Envs: []string{"prod"}, SideEffects: []string{"write_local", "write_remote", "destructive"}}},
			},
		}
		if err := writeJSON(base.PolicyPath, policy); err != nil {
			return nil, err
		}
	}

	if err := seedBuiltinProfiles(base); err != nil {
		return nil, err
	}
	if err := seedBuiltinPacks(base); err != nil {
		return nil, err
	}

	if err := seedBuiltinCapabilites(base); err != nil {
		return nil, err
	}

	if err := seedBuiltinWorkflows(base); err != nil {
		return nil, err
	}

	return LoadState(home)
}
