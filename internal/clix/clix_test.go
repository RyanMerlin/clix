package clix

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSeedStateCreatesStructure(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed state: %v", err)
	}
	for _, p := range []string{state.ConfigPath, state.PolicyPath, state.ProfilesDir, state.CapabilitiesDir, state.WorkflowsDir} {
		if _, err := os.Stat(p); err != nil {
			t.Fatalf("expected %s to exist: %v", p, err)
		}
	}
}

func TestRunBuiltinCapability(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed state: %v", err)
	}
	registry, err := buildRegistry(state)
	if err != nil {
		t.Fatalf("registry: %v", err)
	}
	outcome, err := runCapability(state, registry, state.Policy, "system.date", map[string]any{}, ctxFromState(state), "auto")
	if err != nil {
		t.Fatalf("run capability: %v", err)
	}
	if ok, _ := outcome["ok"].(bool); !ok {
		t.Fatalf("expected success, got %#v", outcome)
	}
}

func TestWorkflowRuns(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed state: %v", err)
	}
	registry, err := buildRegistry(state)
	if err != nil {
		t.Fatalf("registry: %v", err)
	}
	workflows, err := buildWorkflowRegistry(state)
	if err != nil {
		t.Fatalf("workflows: %v", err)
	}
	outcome, err := runWorkflow(state, registry, workflows, state.Policy, "health-check", map[string]any{"message": "ok"}, ctxFromState(state), "auto")
	if err != nil {
		t.Fatalf("workflow run: %v", err)
	}
	if ok, _ := outcome["ok"].(bool); !ok {
		t.Fatalf("expected success, got %#v", outcome)
	}
}

func TestGitStatusDeniedWithoutRepo(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed state: %v", err)
	}
	registry, err := buildRegistry(state)
	if err != nil {
		t.Fatalf("registry: %v", err)
	}
	workspace := filepath.Join(home, "workspace")
	if err := os.MkdirAll(workspace, 0o755); err != nil {
		t.Fatalf("workspace: %v", err)
	}
	outcome, err := runCapability(state, registry, state.Policy, "git.status", map[string]any{"workingDir": workspace}, map[string]string{"cwd": workspace, "env": "local", "profile": "base"}, "auto")
	if err != nil {
		t.Fatalf("run capability: %v", err)
	}
	if ok, _ := outcome["ok"].(bool); ok {
		t.Fatalf("expected denial, got %#v", outcome)
	}
}

func TestPackInstallAndProfileDiscovery(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed state: %v", err)
	}

	source := filepath.Join(home, "source-pack")
	if err := os.MkdirAll(filepath.Join(source, "profiles"), 0o755); err != nil {
		t.Fatalf("source pack: %v", err)
	}
	pack := PackManifest{Name: "sample-pack", Version: 1, Description: "sample"}
	if err := writeJSON(filepath.Join(source, "pack.json"), pack); err != nil {
		t.Fatalf("write pack: %v", err)
	}
	profile := ProfileManifest{Name: "sample-profile", Version: 1, Description: "sample profile"}
	if err := writeJSON(filepath.Join(source, "profiles", "sample-profile.json"), profile); err != nil {
		t.Fatalf("write profile: %v", err)
	}

	installed, err := installPack(source, state.PacksDir, false)
	if err != nil {
		t.Fatalf("install pack: %v", err)
	}
	if installed.Name != "sample-pack" {
		t.Fatalf("unexpected pack: %#v", installed)
	}

	packs, err := loadPackManifests(state.PacksDir)
	if err != nil {
		t.Fatalf("load packs: %v", err)
	}
	foundPack := false
	for _, item := range packs {
		if item.Name == "sample-pack" {
			foundPack = true
			break
		}
	}
	if !foundPack {
		t.Fatalf("installed pack not found: %#v", packs)
	}

	profiles, err := LoadProfiles(state.ProfilesDir)
	if err != nil {
		t.Fatalf("load profiles: %v", err)
	}
	foundProfile := false
	for _, item := range profiles {
		if item.Name == "sample-profile" {
			foundProfile = true
			break
		}
	}
	if !foundProfile {
		t.Fatalf("installed profile not found: %#v", profiles)
	}
}
