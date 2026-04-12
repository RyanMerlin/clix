package clix

import (
	"os"
	"path/filepath"
	"sort"
	"time"
)

type CapabilityRegistry struct {
	items map[string]CapabilityManifest
}

func loadDiskCapabilities(dir string) ([]CapabilityManifest, error) {
	var out []CapabilityManifest
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		var m CapabilityManifest
		if err := readJSON(filepath.Join(dir, entry.Name()), &m); err == nil {
			out = append(out, m)
		}
	}
	return out, nil
}

func loadBuiltinCapabilities() []CapabilityManifest {
	return []CapabilityManifest{
		{Name: "system.date", Version: 1, Description: "Return current time.", Backend: CapabilityBackend{Type: "builtin", Name: "system.date"}, Risk: "low", SideEffectClass: "read_only", SandboxProfile: "none"},
		{Name: "shell.echo", Version: 1, Description: "Echo a supplied message.", Backend: CapabilityBackend{Type: "builtin", Name: "shell.echo"}, Risk: "low", SideEffectClass: "read_only", SandboxProfile: "none"},
		{Name: "node.version", Version: 1, Description: "Return runtime version.", Backend: CapabilityBackend{Type: "builtin", Name: "node.version"}, Risk: "low", SideEffectClass: "read_only", SandboxProfile: "none"},
	}
}

func loadSeededCapabilities() []CapabilityManifest {
	return []CapabilityManifest{
		{Name: "git.status", Version: 1, Description: "Run git status.", Backend: CapabilityBackend{Type: "subprocess", Command: "git", Args: []string{"status", "--short", "--branch"}, CwdFromInput: "workingDir"}, Risk: "low", SideEffectClass: "read_only", SandboxProfile: "workspace_read_only", Validators: []Validator{{Type: "requiredPath", Path: ".git"}}},
		{Name: "gh.version", Version: 1, Description: "Return gh version.", Backend: CapabilityBackend{Type: "subprocess", Command: "gh", Args: []string{"--version"}}, Risk: "low", SideEffectClass: "read_only"},
		{Name: "kubectl.version", Version: 1, Description: "Return kubectl version.", Backend: CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"version", "--client=true"}}, Risk: "low", SideEffectClass: "read_only"},
		{Name: "gcloud.version", Version: 1, Description: "Return gcloud version.", Backend: CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"version"}}, Risk: "low", SideEffectClass: "read_only"},
		{Name: "infisical.version", Version: 1, Description: "Return infisical version.", Backend: CapabilityBackend{Type: "subprocess", Command: "infisical", Args: []string{"--version"}}, Risk: "low", SideEffectClass: "read_only"},
		{Name: "incus.version", Version: 1, Description: "Return incus version.", Backend: CapabilityBackend{Type: "subprocess", Command: "incus", Args: []string{"--version"}}, Risk: "low", SideEffectClass: "read_only"},
		{Name: "argocd.version", Version: 1, Description: "Return argocd version.", Backend: CapabilityBackend{Type: "subprocess", Command: "argocd", Args: []string{"version", "--client"}}, Risk: "low", SideEffectClass: "read_only"},
	}
}

func NewRegistry(caps []CapabilityManifest) *CapabilityRegistry {
	items := map[string]CapabilityManifest{}
	for _, cap := range caps {
		items[cap.Name] = cap
	}
	return &CapabilityRegistry{items: items}
}

func (r *CapabilityRegistry) All() []CapabilityManifest {
	out := make([]CapabilityManifest, 0, len(r.items))
	for _, cap := range r.items {
		out = append(out, cap)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out
}

func (r *CapabilityRegistry) Get(name string) (CapabilityManifest, bool) {
	cap, ok := r.items[name]
	return cap, ok
}

func buildRegistry(state *State) (*CapabilityRegistry, error) {
	caps := append(loadBuiltinCapabilities(), loadSeededCapabilities()...)
	disk, err := loadDiskCapabilities(state.CapabilitiesDir)
	if err != nil {
		return nil, err
	}
	caps = append(caps, disk...)
	return NewRegistry(caps), nil
}

func seedBuiltinCapabilites(base State) error {
	for _, cap := range append(loadBuiltinCapabilities(), loadSeededCapabilities()...) {
		p := filepath.Join(base.CapabilitiesDir, cap.Name+".json")
		if !fileExists(p) {
			if err := writeJSON(p, cap); err != nil {
				return err
			}
		}
	}
	return nil
}

func currentISO() string {
	return nowUTC().Format("2006-01-02T15:04:05.000Z07:00")
}

func nowUTC() time.Time {
	return time.Now().UTC()
}
