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
		{
			Name:        "kubectl-observe",
			Version:     1,
			Description: "Read-only Kubernetes inspection.",
			Capabilities: []CapabilityManifest{
				{Name: "kubectl.version", Version: 1, Description: "Return kubectl version.", Backend: CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"version", "--client=true"}}, Risk: "low", SideEffectClass: "read_only"},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"kubectl.version"}, Profiles: []string{"kubectl-observe"}, SideEffects: []string{"read_only"}}}},
			},
		},
		{
			Name:        "kubectl-change-controlled",
			Version:     1,
			Description: "Guarded Kubernetes change operations.",
			Capabilities: []CapabilityManifest{
				{Name: "kubectl.version", Version: 1, Description: "Return kubectl version.", Backend: CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"version", "--client=true"}}, Risk: "low", SideEffectClass: "read_only"},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"kubectl.version"}, Profiles: []string{"kubectl-change-controlled"}, SideEffects: []string{"read_only"}}}},
			},
		},
		{
			Name:        "gh-readonly",
			Version:     1,
			Description: "GitHub CLI inspection.",
			Capabilities: []CapabilityManifest{
				{Name: "gh.version", Version: 1, Description: "Return gh version.", Backend: CapabilityBackend{Type: "subprocess", Command: "gh", Args: []string{"--version"}}, Risk: "low", SideEffectClass: "read_only"},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"gh.version"}, Profiles: []string{"gh-readonly"}, SideEffects: []string{"read_only"}}}},
			},
		},
		{
			Name:        "git-observer",
			Version:     1,
			Description: "Git workspace inspection.",
			Capabilities: []CapabilityManifest{
				{Name: "git.status", Version: 1, Description: "Run git status.", Backend: CapabilityBackend{Type: "subprocess", Command: "git", Args: []string{"status", "--short", "--branch"}, CwdFromInput: "workingDir"}, Risk: "low", SideEffectClass: "read_only", SandboxProfile: "workspace_read_only", Validators: []Validator{{Type: "requiredPath", Path: ".git"}}},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"git.status"}, Profiles: []string{"git-observer"}, SideEffects: []string{"read_only"}}}},
			},
		},
		{
			Name:        "infisical-readonly",
			Version:     1,
			Description: "Infisical inspection.",
			Capabilities: []CapabilityManifest{
				{Name: "infisical.version", Version: 1, Description: "Return infisical version.", Backend: CapabilityBackend{Type: "subprocess", Command: "infisical", Args: []string{"--version"}}, Risk: "low", SideEffectClass: "read_only"},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"infisical.version"}, Profiles: []string{"infisical-readonly"}, SideEffects: []string{"read_only"}}}},
			},
		},
		{
			Name:        "incus-readonly",
			Version:     1,
			Description: "Incus inspection.",
			Capabilities: []CapabilityManifest{
				{Name: "incus.version", Version: 1, Description: "Return incus version.", Backend: CapabilityBackend{Type: "subprocess", Command: "incus", Args: []string{"--version"}}, Risk: "low", SideEffectClass: "read_only"},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"incus.version"}, Profiles: []string{"incus-readonly"}, SideEffects: []string{"read_only"}}}},
			},
		},
		{
			Name:        "argocd-observe",
			Version:     1,
			Description: "Argo CD inspection.",
			Capabilities: []CapabilityManifest{
				{Name: "argocd.version", Version: 1, Description: "Return argocd version.", Backend: CapabilityBackend{Type: "subprocess", Command: "argocd", Args: []string{"version", "--client"}}, Risk: "low", SideEffectClass: "read_only"},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Capabilities: []string{"argocd.version"}, Profiles: []string{"argocd-observe"}, SideEffects: []string{"read_only"}}}},
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
	rootProfiles, err := loadManifestsFromDir(dir, func(path string) (ProfileManifest, error) {
		var profile ProfileManifest
		if err := readJSON(path, &profile); err != nil {
			return ProfileManifest{}, err
		}
		return profile, nil
	})
	if err != nil {
		return nil, err
	}
	packProfiles, err := loadPackProfiles(filepath.Dir(dir))
	if err != nil {
		return nil, err
	}
	byName := map[string]ProfileManifest{}
	for _, profile := range rootProfiles {
		byName[profile.Name] = profile
	}
	for _, profile := range packProfiles {
		byName[profile.Name] = profile
	}
	out := make([]ProfileManifest, 0, len(byName))
	for _, profile := range byName {
		out = append(out, profile)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}

func loadPackProfiles(packsDir string) ([]ProfileManifest, error) {
	entries, err := os.ReadDir(packsDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []ProfileManifest
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		profilesDir := filepath.Join(packsDir, entry.Name(), "profiles")
		profiles, err := loadManifestsFromDir(profilesDir, func(path string) (ProfileManifest, error) {
			var profile ProfileManifest
			if err := readJSON(path, &profile); err != nil {
				return ProfileManifest{}, err
			}
			return profile, nil
		})
		if err == nil {
			out = append(out, profiles...)
		}
	}
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
