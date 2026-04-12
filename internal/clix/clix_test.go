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

func TestPackScaffoldCreatesTemplate(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed state: %v", err)
	}
	target := filepath.Join(home, "my-pack")
	manifest, err := scaffoldPack(target, "my-pack", "My pack", false)
	if err != nil {
		t.Fatalf("scaffold pack: %v", err)
	}
	if manifest.Name != "my-pack" {
		t.Fatalf("unexpected pack manifest: %#v", manifest)
	}
	for _, p := range []string{
		filepath.Join(target, "pack.json"),
		filepath.Join(target, "profiles", "my-pack.json"),
		filepath.Join(target, "capabilities", "my-pack.version.json"),
		filepath.Join(target, "workflows", "my-pack-health-check.json"),
	} {
		if _, err := os.Stat(p); err != nil {
			t.Fatalf("expected %s to exist: %v", p, err)
		}
	}
	packs, err := loadPackManifests(state.PacksDir)
	if err != nil {
		t.Fatalf("load packs: %v", err)
	}
	if len(packs) == 0 {
		t.Fatalf("expected builtin packs to remain available")
	}
}

func TestPackScaffoldPresets(t *testing.T) {
	cases := []struct {
		name     string
		preset   string
		caps     int
		workflow string
	}{
		{name: "ro-pack", preset: "read-only", caps: 1, workflow: "ro-pack-health-check"},
		{name: "cc-pack", preset: "change-controlled", caps: 2, workflow: "cc-pack-review"},
		{name: "op-pack", preset: "operator", caps: 3, workflow: "op-pack-reconcile"},
	}
	for _, tc := range cases {
		t.Run(tc.preset, func(t *testing.T) {
			home := t.TempDir()
			target := filepath.Join(home, tc.name)
			manifest, err := scaffoldPackWithPreset(target, tc.name, "desc", tc.preset, "", false)
			if err != nil {
				t.Fatalf("scaffold pack: %v", err)
			}
			if len(manifest.Capabilities) != tc.caps {
				t.Fatalf("expected %d capabilities, got %#v", tc.caps, manifest.Capabilities)
			}
			found := false
			for _, wf := range manifest.Workflows {
				if wf == tc.workflow {
					found = true
					break
				}
			}
			if !found {
				t.Fatalf("expected workflow %s in %#v", tc.workflow, manifest.Workflows)
			}
		})
	}
}

func TestPackScaffoldWithCommandBinding(t *testing.T) {
	home := t.TempDir()
	target := filepath.Join(home, "probe-pack")
	manifest, err := scaffoldPackWithPreset(target, "probe-pack", "desc", "read-only", "mycli", false)
	if err != nil {
		t.Fatalf("scaffold pack: %v", err)
	}
	if len(manifest.Capabilities) != 1 {
		t.Fatalf("expected one capability, got %#v", manifest.Capabilities)
	}
	if manifest.Capabilities[0] != "probe-pack.inspect" {
		t.Fatalf("unexpected capability list: %#v", manifest.Capabilities)
	}
	var cap CapabilityManifest
	if err := readJSON(filepath.Join(target, "capabilities", "probe-pack.inspect.json"), &cap); err != nil {
		t.Fatalf("read capability: %v", err)
	}
	if cap.Backend.Command != "mycli" {
		t.Fatalf("unexpected command binding: %#v", cap.Backend)
	}
	if len(cap.Backend.Args) != 1 || cap.Backend.Args[0] != "--help" {
		t.Fatalf("unexpected inspect args: %#v", cap.Backend.Args)
	}
}

func TestOnboardPresetInference(t *testing.T) {
	help := `
Usage: demo [command]

Commands:
  plan        Preview a change
  apply       Apply a change
  status      Show state
  verify      Verify state
`
	preset := inferPackPreset(help)
	if preset != "operator" {
		t.Fatalf("expected operator, got %s", preset)
	}
	commands := extractObservedCommands(help)
	if len(commands) == 0 {
		t.Fatalf("expected commands from help")
	}
	found := false
	for _, name := range commands {
		if name == "plan" {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected plan in %#v", commands)
	}
}
