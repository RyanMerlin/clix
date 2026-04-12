package clix

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

func newID() string {
	var b [16]byte
	_, _ = rand.Read(b[:])
	return hex.EncodeToString(b[:])
}

func runCapability(state *State, registry *CapabilityRegistry, policy PolicyBundle, name string, input map[string]any, ctx map[string]string, approval string) (map[string]any, error) {
	cap, ok := registry.Get(name)
	if !ok {
		return nil, fmt.Errorf("unknown capability: %s", name)
	}
	if input == nil {
		input = map[string]any{}
	}
	execCtx := map[string]string{
		"env":     ctx["env"],
		"cwd":     ctx["cwd"],
		"user":    ctx["user"],
		"profile": ctx["profile"],
	}
	if execCtx["env"] == "" {
		execCtx["env"] = state.Config.DefaultEnv
	}
	if execCtx["cwd"] == "" {
		execCtx["cwd"] = state.Config.WorkspaceRoot
	}
	if execCtx["user"] == "" {
		execCtx["user"] = "unknown"
	}
	if errs := ValidateSchema(cap.InputSchema, input, ""); len(errs) > 0 {
		return nil, fmt.Errorf("input validation failed: %v", errs)
	}
	decision := EvaluatePolicy(policy, execCtx, cap)
	if decision["decision"] == "deny" {
		receipt := map[string]any{
			"id": newID(), "kind": "capability", "capability": cap.Name, "createdAt": currentISO(),
			"status": "rejected", "decision": "deny", "reason": decision["reason"], "input": input, "context": execCtx, "policy": decision,
		}
		_ = appendReceipt(state.ReceiptsDir, receipt)
		return map[string]any{"ok": false, "approvalRequired": false, "receipt": receipt, "reason": decision["reason"], "policy": decision}, nil
	}
	if decision["decision"] == "require_approval" && approval != "auto" {
		receipt := map[string]any{
			"id": newID(), "kind": "capability", "capability": cap.Name, "createdAt": currentISO(),
			"status": "pending_approval", "decision": "require_approval", "reason": decision["reason"], "input": input, "context": execCtx, "policy": decision,
		}
		_ = appendReceipt(state.ReceiptsDir, receipt)
		return map[string]any{"ok": false, "approvalRequired": true, "receipt": receipt, "reason": decision["reason"], "policy": decision}, nil
	}

	resolvedArgs := RenderTemplate(cap.Backend.Args, map[string]any{"input": input, "context": execCtx})
	if valErrs := capabilityValidatorErrors(cap, input, execCtx, resolvedArgs); len(valErrs) > 0 {
		reason := valErrs[0]
		receipt := map[string]any{
			"id": newID(), "kind": "capability", "capability": cap.Name, "createdAt": currentISO(),
			"status": "rejected", "decision": "deny", "reason": reason, "input": input, "context": execCtx, "policy": decision, "errors": valErrs,
		}
		_ = appendReceipt(state.ReceiptsDir, receipt)
		return map[string]any{"ok": false, "approvalRequired": false, "receipt": receipt, "reason": reason, "errors": valErrs}, nil
	}

	var execResult map[string]any
	var err error
	switch cap.Backend.Type {
	case "builtin":
		execResult, err = builtinHandler(cap.Backend.Name, input)
	case "subprocess":
		execResult, err = runSubprocess(cap, execCtx, resolvedArgs)
	case "remote":
		err = errors.New("remote backend not implemented")
	default:
		err = fmt.Errorf("unsupported backend type: %s", cap.Backend.Type)
	}
	if err != nil {
		return nil, err
	}

	success := true
	if exitCode, has := execResult["exitCode"].(int); has {
		success = exitCode == 0
	} else if exitCodeF, has := execResult["exitCode"].(float64); has {
		success = int(exitCodeF) == 0
	}
	receipt := map[string]any{
		"id": newID(), "kind": "capability", "capability": cap.Name, "createdAt": currentISO(),
		"status": map[bool]string{true: "succeeded", false: "failed"}[success], "decision": decision["decision"], "reason": decision["reason"],
		"input": input, "context": execCtx, "policy": decision, "execution": map[string]any{"backend": cap.Backend, "resolvedArgs": resolvedArgs, "result": execResult},
	}
	_ = appendReceipt(state.ReceiptsDir, receipt)
	return map[string]any{"ok": success, "approvalRequired": false, "receipt": receipt, "policy": decision, "result": execResult}, nil
}

func capabilityValidatorErrors(cap CapabilityManifest, input map[string]any, ctx map[string]string, resolvedArgs any) []string {
	var errs []string
	for _, v := range cap.Validators {
		switch v.Type {
		case "requiredPath":
			target := filepath.Join(ctx["cwd"], v.Path)
			if !fileExists(target) {
				errs = append(errs, "Required path missing: "+v.Path)
			}
		case "denyArgs":
			s := fmt.Sprint(resolvedArgs)
			for _, forbidden := range v.Values {
				if strings.Contains(s, forbidden) {
					errs = append(errs, "Forbidden argument detected: "+forbidden)
				}
			}
		case "requiredInputKey":
			if _, ok := input[v.Key]; !ok {
				errs = append(errs, "Input key missing: "+v.Key)
			}
		}
	}
	return errs
}

func runSubprocess(cap CapabilityManifest, ctx map[string]string, resolvedArgs any) (map[string]any, error) {
	args := []string{}
	if list, ok := resolvedArgs.([]any); ok {
		for _, item := range list {
			args = append(args, fmt.Sprint(item))
		}
	} else if list, ok := resolvedArgs.([]string); ok {
		args = append(args, list...)
	}
	cwd := ctx["cwd"]
	if cap.Backend.CwdFromInput != "" {
		cwd = ctx[cap.Backend.CwdFromInput]
	}
	cmd := exec.Command(cap.Backend.Command, args...)
	cmd.Dir = cwd
	out, err := cmd.CombinedOutput()
	result := map[string]any{
		"command":  cap.Backend.Command,
		"args":     args,
		"cwd":      cwd,
		"stdout":   string(out),
		"stderr":   "",
		"payload":  string(out),
		"exitCode": 0,
	}
	if err != nil {
		exitCode := 1
		if ee, ok := err.(*exec.ExitError); ok {
			exitCode = ee.ExitCode()
		}
		result["exitCode"] = exitCode
		result["error"] = err.Error()
	}
	return result, nil
}
