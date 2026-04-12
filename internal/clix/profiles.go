package clix

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

type ResolvedProfile struct {
	Name         string
	Capabilities []CapabilityManifest
	Workflows    []WorkflowManifest
	Policy       PolicyBundle
	Settings     map[string]any
	Source       []string
}

func seedBuiltinProfiles(base State) error {
	profiles := []ProfileManifest{
		{
			Name:        "base",
			Version:     1,
			Description: "Shared defaults and safe read-only capabilities.",
			Capabilities: []CapabilityManifest{
				{Name: "system.date", Version: 1, Description: "Return current time.", Backend: CapabilityBackend{Type: "builtin", Name: "system.date"}, Risk: "low", SideEffectClass: "read_only"},
				{Name: "shell.echo", Version: 1, Description: "Echo a message.", Backend: CapabilityBackend{Type: "builtin", Name: "shell.echo"}, Risk: "low", SideEffectClass: "read_only"},
				{Name: "node.version", Version: 1, Description: "Runtime version.", Backend: CapabilityBackend{Type: "builtin", Name: "node.version"}, Risk: "low", SideEffectClass: "read_only"},
				{Name: "git.status", Version: 1, Description: "Run git status.", Backend: CapabilityBackend{Type: "subprocess", Command: "git", Args: []string{"status", "--short", "--branch"}, CwdFromInput: "workingDir"}, Risk: "low", SideEffectClass: "read_only", SandboxProfile: "workspace_read_only", Validators: []Validator{{Type: "requiredPath", Path: ".git"}}},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules: []PolicyRule{
					{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"system.date", "shell.echo", "node.version", "git.status"}, Profiles: []string{"base"}, SideEffects: []string{"read_only"}}},
				},
			},
		},
		{
			Name:        "gcloud-readonly-planning",
			Version:     1,
			Description: "Read-only gcloud planning and inspection.",
			Capabilities: []CapabilityManifest{
				{Name: "gcloud.version", Version: 1, Description: "Return gcloud version.", Backend: CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"version"}}, Risk: "low", SideEffectClass: "read_only"},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"gcloud.version"}, Profiles: []string{"gcloud-readonly-planning"}, SideEffects: []string{"read_only"}}}},
			},
		},
		{
			Name:        "gcloud-vertex-ai-operator",
			Version:     1,
			Description: "Vertex AI operations profile.",
			Capabilities: []CapabilityManifest{
				{Name: "gcloud.version", Version: 1, Description: "Return gcloud version.", Backend: CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"version"}}, Risk: "low", SideEffectClass: "read_only"},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"gcloud.version"}, Profiles: []string{"gcloud-vertex-ai-operator"}, SideEffects: []string{"read_only"}}}},
			},
		},
	}

	for _, profile := range profiles {
		if err := writeBuiltinProfile(base, profile); err != nil {
			return err
		}
	}
	return nil
}

func writeBuiltinProfile(base State, profile ProfileManifest) error {
	path := filepath.Join(base.ProfilesDir, profile.Name+".json")
	if fileExists(path) {
		return nil
	}
	return writeJSON(path, profile)
}

func LoadProfiles(dir string) ([]ProfileManifest, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	var out []ProfileManifest
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		var profile ProfileManifest
		if err := readJSON(filepath.Join(dir, entry.Name()), &profile); err == nil {
			out = append(out, profile)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}

func (s *State) ActiveProfiles() []string {
	if len(s.Config.ActiveProfiles) == 0 {
		return []string{"base"}
	}
	return s.Config.ActiveProfiles
}

func ResolveProfiles(state *State, names []string, registry *CapabilityRegistry) (*ResolvedProfile, error) {
	if len(names) == 0 {
		names = state.ActiveProfiles()
	}
	profiles, err := LoadProfiles(state.ProfilesDir)
	if err != nil {
		return nil, err
	}
	byName := map[string]ProfileManifest{}
	for _, p := range profiles {
		byName[p.Name] = p
	}
	resolved := &ResolvedProfile{Name: strings.Join(names, ","), Settings: map[string]any{}, Policy: PolicyBundle{DefaultDecision: "deny"}}
	for _, name := range names {
		profile, ok := byName[name]
		if !ok {
			return nil, fmt.Errorf("unknown profile: %s", name)
		}
		resolved.Source = append(resolved.Source, profile.Name)
		if profile.Policy != nil {
			resolved.Policy = mergePolicy(resolved.Policy, *profile.Policy)
		}
		resolved.Settings = mergeMap(resolved.Settings, profile.Settings)
		resolved.Capabilities = mergeCapabilities(resolved.Capabilities, profile.Capabilities)
		resolved.Workflows = mergeWorkflows(resolved.Workflows, profile.Workflows)
	}
	return resolved, nil
}

func mergePolicy(base, overlay PolicyBundle) PolicyBundle {
	if overlay.SchemaVersion != 0 {
		base.SchemaVersion = overlay.SchemaVersion
	}
	if overlay.DefaultDecision != "" {
		base.DefaultDecision = overlay.DefaultDecision
	}
	base.Rules = append(base.Rules, overlay.Rules...)
	return base
}

func mergeMap(base, overlay map[string]any) map[string]any {
	if base == nil {
		base = map[string]any{}
	}
	for k, v := range overlay {
		base[k] = v
	}
	return base
}

func mergeCapabilities(base, overlay []CapabilityManifest) []CapabilityManifest {
	byName := map[string]CapabilityManifest{}
	for _, cap := range base {
		byName[cap.Name] = cap
	}
	for _, cap := range overlay {
		byName[cap.Name] = cap
	}
	out := make([]CapabilityManifest, 0, len(byName))
	for _, cap := range byName {
		out = append(out, cap)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out
}

func mergeWorkflows(base, overlay []WorkflowManifest) []WorkflowManifest {
	byName := map[string]WorkflowManifest{}
	for _, wf := range base {
		byName[wf.Name] = wf
	}
	for _, wf := range overlay {
		byName[wf.Name] = wf
	}
	out := make([]WorkflowManifest, 0, len(byName))
	for _, wf := range byName {
		out = append(out, wf)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out
}
