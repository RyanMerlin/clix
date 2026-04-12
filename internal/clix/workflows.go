package clix

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
)

func seedBuiltinWorkflows(base State) error {
	wf := WorkflowManifest{
		Name:        "health-check",
		Version:     1,
		Description: "Small end-to-end health check.",
		InputSchema: map[string]any{
			"type": "object",
			"properties": map[string]any{
				"message":    map[string]any{"type": "string"},
				"workingDir": map[string]any{"type": "string"},
			},
		},
		Steps: []WorkflowStep{
			{Name: "date", Capability: "system.date"},
			{Name: "echo", Capability: "shell.echo", Input: map[string]any{"message": "health-check:${inputs.message}"}},
		},
	}
	path := filepath.Join(base.WorkflowsDir, wf.Name+".json")
	if !fileExists(path) {
		return writeJSON(path, wf)
	}
	return nil
}

func loadWorkflows(dir string) ([]WorkflowManifest, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	var out []WorkflowManifest
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		var wf WorkflowManifest
		if err := readJSON(filepath.Join(dir, entry.Name()), &wf); err == nil {
			out = append(out, wf)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}

type WorkflowRegistry struct {
	items map[string]WorkflowManifest
}

func newWorkflowRegistry(list []WorkflowManifest) *WorkflowRegistry {
	items := map[string]WorkflowManifest{}
	for _, wf := range list {
		items[wf.Name] = wf
	}
	return &WorkflowRegistry{items: items}
}

func (r *WorkflowRegistry) All() []WorkflowManifest {
	out := make([]WorkflowManifest, 0, len(r.items))
	for _, wf := range r.items {
		out = append(out, wf)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out
}

func (r *WorkflowRegistry) Get(name string) (WorkflowManifest, bool) {
	wf, ok := r.items[name]
	return wf, ok
}

func buildWorkflowRegistry(state *State) (*WorkflowRegistry, error) {
	wfs, err := loadWorkflows(state.WorkflowsDir)
	if err != nil {
		return nil, err
	}
	return newWorkflowRegistry(wfs), nil
}

func runWorkflow(state *State, registry *CapabilityRegistry, workflows *WorkflowRegistry, policy PolicyBundle, name string, input map[string]any, ctx map[string]string, approval string) (map[string]any, error) {
	wf, ok := workflows.Get(name)
	if !ok {
		return nil, fmt.Errorf("unknown workflow: %s", name)
	}
	if errs := ValidateSchema(wf.InputSchema, input, ""); len(errs) > 0 {
		return nil, fmt.Errorf("workflow input validation failed: %v", errs)
	}
	result := map[string]any{
		"id":        newID(),
		"kind":      "workflow",
		"workflow":  wf.Name,
		"createdAt": currentISO(),
		"status":    "running",
		"input":     input,
		"context":   ctx,
		"steps":     []any{},
	}
	scope := map[string]any{"inputs": input, "state": map[string]any{}}
	var stepResults []any
	for _, step := range wf.Steps {
		stepInput := RenderTemplate(step.Input, scope)
		inputMap, _ := stepInput.(map[string]any)
		outcome, err := runCapability(state, registry, policy, step.Capability, inputMap, ctx, approval)
		if err != nil {
			return nil, err
		}
		stepResults = append(stepResults, map[string]any{
			"name":       step.Name,
			"capability": step.Capability,
			"outcome":    outcome,
		})
		scope["state"].(map[string]any)[step.Name] = outcome["result"]
		result["steps"] = append(result["steps"].([]any), map[string]any{
			"name":             step.Name,
			"capability":       step.Capability,
			"receiptId":        outcome["receipt"].(map[string]any)["id"],
			"ok":               outcome["ok"],
			"approvalRequired": outcome["approvalRequired"],
		})
		if !boolValue(outcome["ok"]) || boolValue(outcome["approvalRequired"]) {
			result["status"] = map[bool]string{true: "pending_approval", false: "failed"}[boolValue(outcome["approvalRequired"])]
			result["finishedAt"] = currentISO()
			result["error"] = outcome["reason"]
			_ = appendReceipt(state.ReceiptsDir, result)
			return map[string]any{"ok": false, "approvalRequired": outcome["approvalRequired"], "receipt": result, "results": stepResults, "error": outcome["reason"]}, nil
		}
	}
	result["status"] = "succeeded"
	result["finishedAt"] = currentISO()
	_ = appendReceipt(state.ReceiptsDir, result)
	return map[string]any{"ok": true, "approvalRequired": false, "receipt": result, "results": stepResults}, nil
}

func boolValue(v any) bool {
	b, _ := v.(bool)
	return b
}
