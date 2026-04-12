package clix

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

func loadManifestsFromDir[T any](dir string, fn func(string) (T, error)) ([]T, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []T
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		item, err := fn(filepath.Join(dir, entry.Name()))
		if err == nil {
			out = append(out, item)
		}
	}
	return out, nil
}

func loadPackManifests(dir string) ([]PackManifest, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []PackManifest
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		packPath := filepath.Join(dir, entry.Name(), "pack.json")
		var manifest PackManifest
		if err := readJSON(packPath, &manifest); err == nil {
			out = append(out, manifest)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}

func seedBuiltinPack(base State, pack PackManifest) error {
	path := filepath.Join(base.PacksDir, pack.Name)
	if fileExists(path) {
		return nil
	}
	if err := ensureDir(path); err != nil {
		return err
	}
	return writeJSON(filepath.Join(path, "pack.json"), pack)
}

func seedBuiltinPacks(base State) error {
	packs := []PackManifest{
		{Name: "base", Version: 1, Description: "Shared safe defaults.", Profiles: []string{"base"}},
		{Name: "gcloud-readonly-planning", Version: 1, Description: "Read-only gcloud planning.", Profiles: []string{"gcloud-readonly-planning"}},
		{Name: "gcloud-vertex-ai-operator", Version: 1, Description: "Vertex AI operations.", Profiles: []string{"gcloud-vertex-ai-operator"}},
		{Name: "kubectl-observe", Version: 1, Description: "Read-only kubectl inspection.", Profiles: []string{"kubectl-observe"}},
		{Name: "kubectl-change-controlled", Version: 1, Description: "Guarded kubectl change operations.", Profiles: []string{"kubectl-change-controlled"}},
		{Name: "gh-readonly", Version: 1, Description: "Read-only GitHub CLI inspection.", Profiles: []string{"gh-readonly"}},
		{Name: "git-observer", Version: 1, Description: "Git workspace inspection.", Profiles: []string{"git-observer"}},
		{Name: "infisical-readonly", Version: 1, Description: "Read-only Infisical inspection.", Profiles: []string{"infisical-readonly"}},
		{Name: "incus-readonly", Version: 1, Description: "Read-only Incus inspection.", Profiles: []string{"incus-readonly"}},
		{Name: "argocd-observe", Version: 1, Description: "Read-only Argo CD inspection.", Profiles: []string{"argocd-observe"}},
	}
	for _, pack := range packs {
		if err := seedBuiltinPack(base, pack); err != nil {
			return err
		}
	}
	for _, pack := range packs {
		if err := seedBuiltinPackContents(base, pack); err != nil {
			return err
		}
	}
	return nil
}

func seedBuiltinPackContents(base State, pack PackManifest) error {
	dir := filepath.Join(base.PacksDir, pack.Name)
	switch pack.Name {
	case "gcloud-readonly-planning":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "gcloud-readonly-planning",
			Version:     1,
			Description: "Read-only gcloud planning and inspection.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "gcloud.version",
					Version:         1,
					Description:     "Return gcloud version.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"version"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "gcloud.projects.list",
					Version:         1,
					Description:     "List Google Cloud projects.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"projects", "list", "--format=json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "gcloud.config.list",
					Version:         1,
					Description:     "List Google Cloud configuration.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"config", "list", "--format=json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules: []PolicyRule{
					{Effect: "allow", Match: PolicyMatch{Profiles: []string{"gcloud-readonly-planning"}, SideEffects: []string{"read_only"}}},
				},
			},
		}, []WorkflowManifest{
			{
				Name:        "gcloud-readonly-inventory",
				Version:     1,
				Description: "Gather a basic read-only Google Cloud snapshot.",
				Steps: []WorkflowStep{
					{Name: "version", Capability: "gcloud.version"},
					{Name: "projects", Capability: "gcloud.projects.list"},
					{Name: "config", Capability: "gcloud.config.list"},
				},
			},
		}, nil)
	case "gcloud-vertex-ai-operator":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "gcloud-vertex-ai-operator",
			Version:     1,
			Description: "Vertex AI operations profile.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "gcloud.version",
					Version:         1,
					Description:     "Return gcloud version.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"version"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "gcloud.ai.models.list",
					Version:         1,
					Description:     "List Vertex AI models in a region.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"ai", "models", "list", "--region=${input.region}", "--format=json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"region"},
						"additionalProperties": false,
						"properties": map[string]any{
							"region": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "region"}},
				},
				{
					Name:            "gcloud.ai.endpoints.list",
					Version:         1,
					Description:     "List Vertex AI endpoints in a region.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gcloud", Args: []string{"ai", "endpoints", "list", "--region=${input.region}", "--format=json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"region"},
						"additionalProperties": false,
						"properties": map[string]any{
							"region": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "region"}},
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules: []PolicyRule{
					{Effect: "allow", Match: PolicyMatch{Profiles: []string{"gcloud-vertex-ai-operator"}, SideEffects: []string{"read_only"}}},
				},
			},
		}, []WorkflowManifest{
			{
				Name:        "vertex-ai-inventory",
				Version:     1,
				Description: "Inspect Vertex AI models and endpoints in a region.",
				InputSchema: map[string]any{
					"type":                 "object",
					"required":             []any{"region"},
					"additionalProperties": false,
					"properties": map[string]any{
						"region": map[string]any{"type": "string"},
					},
				},
				Steps: []WorkflowStep{
					{Name: "version", Capability: "gcloud.version"},
					{Name: "models", Capability: "gcloud.ai.models.list", Input: map[string]any{"region": "${inputs.region}"}},
					{Name: "endpoints", Capability: "gcloud.ai.endpoints.list", Input: map[string]any{"region": "${inputs.region}"}},
				},
			},
		}, nil)
	case "kubectl-observe":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "kubectl-observe",
			Version:     1,
			Description: "Read-only Kubernetes inspection.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "kubectl.version",
					Version:         1,
					Description:     "Return kubectl version.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"version", "--client=true"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "kubectl.get.pods",
					Version:         1,
					Description:     "List pods across all namespaces.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"get", "pods", "-A", "-o", "json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "kubectl.get.deployments",
					Version:         1,
					Description:     "List deployments across all namespaces.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"get", "deployments", "-A", "-o", "json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "kubectl.get.events",
					Version:         1,
					Description:     "List events across all namespaces.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"get", "events", "-A", "-o", "json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules: []PolicyRule{
					{Effect: "allow", Match: PolicyMatch{Profiles: []string{"kubectl-observe"}, SideEffects: []string{"read_only"}}},
				},
			},
		}, []WorkflowManifest{
			{
				Name:        "cluster-overview",
				Version:     1,
				Description: "Collect a basic cluster overview.",
				Steps: []WorkflowStep{
					{Name: "version", Capability: "kubectl.version"},
					{Name: "pods", Capability: "kubectl.get.pods"},
					{Name: "deployments", Capability: "kubectl.get.deployments"},
				},
			},
		}, nil)
	case "kubectl-change-controlled":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "kubectl-change-controlled",
			Version:     1,
			Description: "Guarded Kubernetes change operations.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "kubectl.version",
					Version:         1,
					Description:     "Return kubectl version.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"version", "--client=true"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "kubectl.diff",
					Version:         1,
					Description:     "Show diff for a manifest.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"diff", "-f", "${input.manifest}"}},
					Risk:            "medium",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"manifest"},
						"additionalProperties": false,
						"properties": map[string]any{
							"manifest": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "manifest"}},
				},
				{
					Name:            "kubectl.apply",
					Version:         1,
					Description:     "Apply a manifest with guarded approval.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "kubectl", Args: []string{"apply", "-f", "${input.manifest}"}},
					Risk:            "high",
					SideEffectClass: "write_remote",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"manifest"},
						"additionalProperties": false,
						"properties": map[string]any{
							"manifest": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "manifest"}},
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules: []PolicyRule{
					{Effect: "require_approval", Match: PolicyMatch{Capabilities: []string{"kubectl.diff"}, Profiles: []string{"kubectl-change-controlled"}}},
					{Effect: "require_approval", Match: PolicyMatch{Capabilities: []string{"kubectl.apply"}, Profiles: []string{"kubectl-change-controlled"}, Risk: []string{"high"}}},
				},
			},
		}, []WorkflowManifest{
			{
				Name:        "change-review",
				Version:     1,
				Description: "Review a manifest change before apply.",
				InputSchema: map[string]any{
					"type":                 "object",
					"required":             []any{"manifest"},
					"additionalProperties": false,
					"properties": map[string]any{
						"manifest": map[string]any{"type": "string"},
					},
				},
				Steps: []WorkflowStep{
					{Name: "version", Capability: "kubectl.version"},
					{Name: "diff", Capability: "kubectl.diff", Input: map[string]any{"manifest": "${inputs.manifest}"}},
				},
			},
		}, nil)
	case "gh-readonly":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "gh-readonly",
			Version:     1,
			Description: "GitHub CLI inspection.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "gh.version",
					Version:         1,
					Description:     "Return gh version.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gh", Args: []string{"--version"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "gh.repo.view",
					Version:         1,
					Description:     "Inspect a repository.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gh", Args: []string{"repo", "view", "${input.repo}", "--json", "name,description,defaultBranchRef,visibility"}},
					Risk:            "low",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"repo"},
						"additionalProperties": false,
						"properties": map[string]any{
							"repo": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "repo"}},
				},
				{
					Name:            "gh.issue.list",
					Version:         1,
					Description:     "List issues in a repository.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gh", Args: []string{"issue", "list", "--repo", "${input.repo}", "--limit", "${input.limit}", "--json", "number,title,state"}},
					Risk:            "low",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"repo"},
						"additionalProperties": false,
						"properties": map[string]any{
							"repo":  map[string]any{"type": "string"},
							"limit": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "repo"}},
				},
				{
					Name:            "gh.pr.list",
					Version:         1,
					Description:     "List pull requests in a repository.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "gh", Args: []string{"pr", "list", "--repo", "${input.repo}", "--limit", "${input.limit}", "--json", "number,title,state,headRefName,baseRefName"}},
					Risk:            "low",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"repo"},
						"additionalProperties": false,
						"properties": map[string]any{
							"repo":  map[string]any{"type": "string"},
							"limit": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "repo"}},
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Profiles: []string{"gh-readonly"}, SideEffects: []string{"read_only"}}}},
			},
		}, []WorkflowManifest{
			{
				Name:        "repo-intel",
				Version:     1,
				Description: "Collect repository metadata and open work.",
				InputSchema: map[string]any{
					"type":                 "object",
					"required":             []any{"repo"},
					"additionalProperties": false,
					"properties": map[string]any{
						"repo":  map[string]any{"type": "string"},
						"limit": map[string]any{"type": "string"},
					},
				},
				Steps: []WorkflowStep{
					{Name: "version", Capability: "gh.version"},
					{Name: "repo", Capability: "gh.repo.view", Input: map[string]any{"repo": "${inputs.repo}"}},
					{Name: "issues", Capability: "gh.issue.list", Input: map[string]any{"repo": "${inputs.repo}", "limit": "${inputs.limit}"}},
				},
			},
		}, nil)
	case "git-observer":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "git-observer",
			Version:     1,
			Description: "Git workspace inspection.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "git.status",
					Version:         1,
					Description:     "Run git status.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "git", Args: []string{"status", "--short", "--branch"}, CwdFromInput: "workingDir"},
					Risk:            "low",
					SideEffectClass: "read_only",
					SandboxProfile:  "workspace_read_only",
					Validators:      []Validator{{Type: "requiredPath", Path: ".git"}},
				},
				{
					Name:            "git.diff",
					Version:         1,
					Description:     "Show git diff.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "git", Args: []string{"diff"}, CwdFromInput: "workingDir"},
					Risk:            "low",
					SideEffectClass: "read_only",
					SandboxProfile:  "workspace_read_only",
					Validators:      []Validator{{Type: "requiredPath", Path: ".git"}},
				},
				{
					Name:            "git.log",
					Version:         1,
					Description:     "Show recent git history.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "git", Args: []string{"log", "--oneline", "--decorate", "-n", "${input.limit}"}, CwdFromInput: "workingDir"},
					Risk:            "low",
					SideEffectClass: "read_only",
					SandboxProfile:  "workspace_read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"workingDir"},
						"additionalProperties": false,
						"properties": map[string]any{
							"workingDir": map[string]any{"type": "string"},
							"limit":      map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredPath", Path: ".git"}},
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Profiles: []string{"git-observer"}, SideEffects: []string{"read_only"}}}},
			},
		}, []WorkflowManifest{
			{
				Name:        "workspace-overview",
				Version:     1,
				Description: "Gather a quick git workspace snapshot.",
				InputSchema: map[string]any{
					"type":                 "object",
					"required":             []any{"workingDir"},
					"additionalProperties": false,
					"properties": map[string]any{
						"workingDir": map[string]any{"type": "string"},
						"limit":      map[string]any{"type": "string"},
					},
				},
				Steps: []WorkflowStep{
					{Name: "status", Capability: "git.status", Input: map[string]any{"workingDir": "${inputs.workingDir}"}},
					{Name: "diff", Capability: "git.diff", Input: map[string]any{"workingDir": "${inputs.workingDir}"}},
				},
			},
		}, nil)
	case "infisical-readonly":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "infisical-readonly",
			Version:     1,
			Description: "Infisical inspection.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "infisical.version",
					Version:         1,
					Description:     "Return infisical version.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "infisical", Args: []string{"--version"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "infisical.secrets.list",
					Version:         1,
					Description:     "List secrets in an environment.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "infisical", Args: []string{"secrets", "list", "--projectId", "${input.projectId}", "--env", "${input.env}", "--format", "json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"projectId", "env"},
						"additionalProperties": false,
						"properties": map[string]any{
							"projectId": map[string]any{"type": "string"},
							"env":       map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "projectId"}, {Type: "requiredInputKey", Key: "env"}},
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Profiles: []string{"infisical-readonly"}, SideEffects: []string{"read_only"}}}},
			},
		}, []WorkflowManifest{
			{
				Name:        "secret-inventory",
				Version:     1,
				Description: "Collect a secret inventory for a project and environment.",
				InputSchema: map[string]any{
					"type":                 "object",
					"required":             []any{"projectId", "env"},
					"additionalProperties": false,
					"properties": map[string]any{
						"projectId": map[string]any{"type": "string"},
						"env":       map[string]any{"type": "string"},
					},
				},
				Steps: []WorkflowStep{
					{Name: "version", Capability: "infisical.version"},
					{Name: "secrets", Capability: "infisical.secrets.list", Input: map[string]any{"projectId": "${inputs.projectId}", "env": "${inputs.env}"}},
				},
			},
		}, nil)
	case "incus-readonly":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "incus-readonly",
			Version:     1,
			Description: "Incus inspection.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "incus.version",
					Version:         1,
					Description:     "Return incus version.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "incus", Args: []string{"--version"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "incus.list",
					Version:         1,
					Description:     "List Incus instances.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "incus", Args: []string{"list", "--format=json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "incus.info",
					Version:         1,
					Description:     "Show Incus instance info.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "incus", Args: []string{"info", "${input.name}", "--format=json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"name"},
						"additionalProperties": false,
						"properties": map[string]any{
							"name": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "name"}},
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Profiles: []string{"incus-readonly"}, SideEffects: []string{"read_only"}}}},
			},
		}, []WorkflowManifest{
			{
				Name:        "instance-overview",
				Version:     1,
				Description: "Inspect Incus state.",
				Steps: []WorkflowStep{
					{Name: "version", Capability: "incus.version"},
					{Name: "list", Capability: "incus.list"},
				},
			},
		}, nil)
	case "argocd-observe":
		return seedPackBundle(dir, pack, ProfileManifest{
			Name:        "argocd-observe",
			Version:     1,
			Description: "Argo CD inspection.",
			Capabilities: []CapabilityManifest{
				{
					Name:            "argocd.version",
					Version:         1,
					Description:     "Return argocd version.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "argocd", Args: []string{"version", "--client"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "argocd.app.list",
					Version:         1,
					Description:     "List Argo CD applications.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "argocd", Args: []string{"app", "list", "-o", "json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
				},
				{
					Name:            "argocd.app.get",
					Version:         1,
					Description:     "Inspect an Argo CD application.",
					Backend:         CapabilityBackend{Type: "subprocess", Command: "argocd", Args: []string{"app", "get", "${input.app}", "-o", "json"}},
					Risk:            "low",
					SideEffectClass: "read_only",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"app"},
						"additionalProperties": false,
						"properties": map[string]any{
							"app": map[string]any{"type": "string"},
						},
					},
					Validators: []Validator{{Type: "requiredInputKey", Key: "app"}},
				},
			},
			Policy: &PolicyBundle{
				SchemaVersion:   1,
				DefaultDecision: "deny",
				Rules:           []PolicyRule{{Effect: "allow", Match: PolicyMatch{Profiles: []string{"argocd-observe"}, SideEffects: []string{"read_only"}}}},
			},
		}, []WorkflowManifest{
			{
				Name:        "application-overview",
				Version:     1,
				Description: "Collect a quick Argo CD application snapshot.",
				InputSchema: map[string]any{
					"type":                 "object",
					"required":             []any{"app"},
					"additionalProperties": false,
					"properties": map[string]any{
						"app": map[string]any{"type": "string"},
					},
				},
				Steps: []WorkflowStep{
					{Name: "version", Capability: "argocd.version"},
					{Name: "apps", Capability: "argocd.app.list"},
					{Name: "app", Capability: "argocd.app.get", Input: map[string]any{"app": "${inputs.app}"}},
				},
			},
		}, nil)
	default:
		return nil
	}
}

func seedPackBundle(dir string, pack PackManifest, profile ProfileManifest, workflows []WorkflowManifest, extras []CapabilityManifest) error {
	if err := ensureDir(filepath.Join(dir, "profiles")); err != nil {
		return err
	}
	if err := ensureDir(filepath.Join(dir, "capabilities")); err != nil {
		return err
	}
	if err := ensureDir(filepath.Join(dir, "workflows")); err != nil {
		return err
	}
	if err := writeJSON(filepath.Join(dir, "pack.json"), pack); err != nil {
		return err
	}
	if err := writeJSON(filepath.Join(dir, "profiles", profile.Name+".json"), profile); err != nil {
		return err
	}
	for _, cap := range profile.Capabilities {
		if err := writeJSON(filepath.Join(dir, "capabilities", cap.Name+".json"), cap); err != nil {
			return err
		}
	}
	for _, cap := range extras {
		if err := writeJSON(filepath.Join(dir, "capabilities", cap.Name+".json"), cap); err != nil {
			return err
		}
	}
	for _, wf := range workflows {
		if err := writeJSON(filepath.Join(dir, "workflows", wf.Name+".json"), wf); err != nil {
			return err
		}
	}
	return nil
}

func copyDir(src, dst string) error {
	entries, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	if err := ensureDir(dst); err != nil {
		return err
	}
	for _, entry := range entries {
		sourcePath := filepath.Join(src, entry.Name())
		targetPath := filepath.Join(dst, entry.Name())
		if entry.IsDir() {
			if err := copyDir(sourcePath, targetPath); err != nil {
				return err
			}
			continue
		}
		in, err := os.Open(sourcePath)
		if err != nil {
			return err
		}
		out, err := os.Create(targetPath)
		if err != nil {
			in.Close()
			return err
		}
		if _, err := io.Copy(out, in); err != nil {
			in.Close()
			out.Close()
			return err
		}
		if err := in.Close(); err != nil {
			out.Close()
			return err
		}
		if err := out.Close(); err != nil {
			return err
		}
	}
	return nil
}

func scaffoldPack(targetDir, name, description string, force bool) (PackManifest, error) {
	return scaffoldPackWithPreset(targetDir, name, description, "read-only", "", force)
}

func scaffoldPackWithPreset(targetDir, name, description, preset, command string, force bool) (PackManifest, error) {
	if name == "" {
		return PackManifest{}, fmt.Errorf("pack name is required")
	}
	if description == "" {
		description = "Generated pack scaffold."
	}
	spec, err := packScaffoldSpec(name, description, preset, command)
	if err != nil {
		return PackManifest{}, err
	}
	manifest := PackManifest{
		Name:         name,
		Version:      1,
		Description:  description,
		Profiles:     []string{name},
		Capabilities: spec.capabilityNames(),
		Workflows:    spec.workflowNames(),
	}

	if fileExists(targetDir) {
		if !force {
			return PackManifest{}, fmt.Errorf("target already exists: %s", targetDir)
		}
		if err := os.RemoveAll(targetDir); err != nil {
			return PackManifest{}, err
		}
	}
	for _, dir := range []string{
		targetDir,
		filepath.Join(targetDir, "profiles"),
		filepath.Join(targetDir, "capabilities"),
		filepath.Join(targetDir, "workflows"),
		filepath.Join(targetDir, "plugins"),
	} {
		if err := ensureDir(dir); err != nil {
			return PackManifest{}, err
		}
	}
	if err := writeJSON(filepath.Join(targetDir, "pack.json"), manifest); err != nil {
		return PackManifest{}, err
	}

	profile := spec.profile
	if err := writeJSON(filepath.Join(targetDir, "profiles", name+".json"), profile); err != nil {
		return PackManifest{}, err
	}

	for _, capability := range spec.capabilities {
		if err := writeJSON(filepath.Join(targetDir, "capabilities", capability.Name+".json"), capability); err != nil {
			return PackManifest{}, err
		}
	}

	for _, workflow := range spec.workflows {
		if err := writeJSON(filepath.Join(targetDir, "workflows", workflow.Name+".json"), workflow); err != nil {
			return PackManifest{}, err
		}
	}

	readmeParts := []string{
		fmt.Sprintf(`# %s`, name),
		"",
		description,
		"",
		"## Contents",
		"",
		"- pack manifest: pack.json",
		"- profile: profiles/" + name + ".json",
	}
	if command != "" {
		readmeParts = append(readmeParts,
			"- command binding: "+command,
			"- onboarding report: onboard.json",
		)
	}
	readmeParts = append(readmeParts,
		"- capabilities:",
		indentBulletList(spec.capabilityNames()),
		"- workflows:",
		indentBulletList(spec.workflowNames()),
	)
	readme := strings.TrimSpace(strings.Join(readmeParts, "\n"))
	if err := os.WriteFile(filepath.Join(targetDir, "README.md"), []byte(readme+"\n"), 0o644); err != nil {
		return PackManifest{}, err
	}

	return manifest, nil
}

type packScaffoldTemplate struct {
	profile      ProfileManifest
	capabilities []CapabilityManifest
	workflows    []WorkflowManifest
}

func (t packScaffoldTemplate) capabilityNames() []string {
	names := make([]string, 0, len(t.capabilities))
	for _, capability := range t.capabilities {
		names = append(names, capability.Name)
	}
	return names
}

func (t packScaffoldTemplate) workflowNames() []string {
	names := make([]string, 0, len(t.workflows))
	for _, workflow := range t.workflows {
		names = append(names, workflow.Name)
	}
	return names
}

func packScaffoldSpec(name, description, preset, command string) (packScaffoldTemplate, error) {
	baseProfile := ProfileManifest{
		Name:        name,
		Version:     1,
		Description: description,
		Policy: &PolicyBundle{
			SchemaVersion:   1,
			DefaultDecision: "deny",
			Rules: []PolicyRule{
				{Effect: "allow", Match: PolicyMatch{Profiles: []string{name}, SideEffects: []string{"read_only"}}},
			},
		},
		Settings: map[string]any{
			"packName": name,
			"preset":   preset,
		},
	}
	if command != "" {
		baseProfile.Settings["command"] = command
	}

	switch preset {
	case "read-only":
		stepName := "version"
		cap := CapabilityManifest{
			Name:            name + ".version",
			Version:         1,
			Description:     "Return version information for the pack.",
			Backend:         CapabilityBackend{Type: "builtin", Name: "node.version"},
			Risk:            "low",
			SideEffectClass: "read_only",
		}
		if command != "" {
			stepName = "inspect"
			cap.Name = name + ".inspect"
			cap.Description = "Inspect the CLI help output."
			cap.Backend = CapabilityBackend{Type: "subprocess", Command: command, Args: []string{"--help"}}
		}
		baseProfile.Capabilities = []CapabilityManifest{cap}
		return packScaffoldTemplate{
			profile:      baseProfile,
			capabilities: []CapabilityManifest{cap},
			workflows: []WorkflowManifest{
				{
					Name:        name + "-health-check",
					Version:     1,
					Description: "Basic read-only scaffold workflow.",
					Steps:       []WorkflowStep{{Name: stepName, Capability: cap.Name}},
				},
			},
		}, nil
	case "change-controlled":
		binary := "replace-me"
		if command != "" {
			binary = command
		}
		plan := CapabilityManifest{
			Name:            name + ".plan",
			Version:         1,
			Description:     "Plan a change without applying it.",
			Backend:         CapabilityBackend{Type: "subprocess", Command: binary, Args: []string{"plan"}},
			Risk:            "medium",
			SideEffectClass: "read_only",
			Validators:      []Validator{{Type: "requiredInputKey", Key: "target"}},
		}
		apply := CapabilityManifest{
			Name:            name + ".apply",
			Version:         1,
			Description:     "Apply a reviewed change.",
			Backend:         CapabilityBackend{Type: "subprocess", Command: binary, Args: []string{"apply"}},
			Risk:            "high",
			SideEffectClass: "write_remote",
			Validators:      []Validator{{Type: "requiredInputKey", Key: "target"}},
		}
		baseProfile.Capabilities = []CapabilityManifest{plan, apply}
		baseProfile.Policy.Rules = append(baseProfile.Policy.Rules,
			PolicyRule{Effect: "require_approval", Match: PolicyMatch{Capabilities: []string{apply.Name}, Profiles: []string{name}, Risk: []string{"high"}}},
		)
		return packScaffoldTemplate{
			profile:      baseProfile,
			capabilities: []CapabilityManifest{plan, apply},
			workflows: []WorkflowManifest{
				{
					Name:        name + "-review",
					Version:     1,
					Description: "Plan before apply.",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"target"},
						"additionalProperties": false,
						"properties": map[string]any{
							"target": map[string]any{"type": "string"},
						},
					},
					Steps: []WorkflowStep{
						{Name: "plan", Capability: plan.Name, Input: map[string]any{"target": "${inputs.target}"}},
					},
				},
			},
		}, nil
	case "operator":
		binary := "replace-me"
		if command != "" {
			binary = command
		}
		status := CapabilityManifest{
			Name:            name + ".status",
			Version:         1,
			Description:     "Inspect current pack state.",
			Backend:         CapabilityBackend{Type: "builtin", Name: "system.date"},
			Risk:            "low",
			SideEffectClass: "read_only",
		}
		reconcile := CapabilityManifest{
			Name:            name + ".reconcile",
			Version:         1,
			Description:     "Reconcile the pack's intended state.",
			Backend:         CapabilityBackend{Type: "subprocess", Command: binary, Args: []string{"reconcile"}},
			Risk:            "medium",
			SideEffectClass: "write_local",
			Validators:      []Validator{{Type: "requiredInputKey", Key: "target"}},
		}
		confirm := CapabilityManifest{
			Name:            name + ".verify",
			Version:         1,
			Description:     "Verify the pack state after reconcile.",
			Backend:         CapabilityBackend{Type: "builtin", Name: "system.date"},
			Risk:            "low",
			SideEffectClass: "read_only",
		}
		baseProfile.Capabilities = []CapabilityManifest{status, reconcile, confirm}
		baseProfile.Policy.Rules = append(baseProfile.Policy.Rules,
			PolicyRule{Effect: "require_approval", Match: PolicyMatch{Capabilities: []string{reconcile.Name}, Profiles: []string{name}, SideEffects: []string{"write_local"}}},
		)
		return packScaffoldTemplate{
			profile:      baseProfile,
			capabilities: []CapabilityManifest{status, reconcile, confirm},
			workflows: []WorkflowManifest{
				{
					Name:        name + "-reconcile",
					Version:     1,
					Description: "Inspect, reconcile, then verify.",
					InputSchema: map[string]any{
						"type":                 "object",
						"required":             []any{"target"},
						"additionalProperties": false,
						"properties": map[string]any{
							"target": map[string]any{"type": "string"},
						},
					},
					Steps: []WorkflowStep{
						{Name: "status", Capability: status.Name},
						{Name: "reconcile", Capability: reconcile.Name, Input: map[string]any{"target": "${inputs.target}"}},
						{Name: "verify", Capability: confirm.Name},
					},
				},
			},
		}, nil
	default:
		return packScaffoldTemplate{}, fmt.Errorf("unknown preset: %s", preset)
	}
}

func indentBulletList(items []string) string {
	if len(items) == 0 {
		return "  - none"
	}
	var b strings.Builder
	for _, item := range items {
		b.WriteString("  - ")
		b.WriteString(item)
		b.WriteString("\n")
	}
	return strings.TrimRight(b.String(), "\n")
}
