package clix

import (
	"crypto/rand"
	"encoding/hex"
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
		execResult, err = runSubprocess(cap, execCtx, resolvedArgs, state.Config.Infisical)
	case "remote":
		execResult, err = runRemote(cap, input)
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

func runSubprocess(cap CapabilityManifest, ctx map[string]string, resolvedArgs any, infisicalCfg InfisicalConfig) (map[string]any, error) {
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

	// Resolve credentials before launching the subprocess.
	var secrets map[string]string
	if len(cap.Backend.Credentials) > 0 {
		var err error
		secrets, err = resolveCredentials(cap.Backend.Credentials, infisicalCfg)
		if err != nil {
			return nil, fmt.Errorf("credential resolution failed: %w", err)
		}
	}

	cmd := exec.Command(cap.Backend.Command, args...)
	cmd.Dir = cwd
	cmd.Env = buildSubprocessEnv(secrets)
	out, err := cmd.CombinedOutput()

	// Redact credential values from output before they can leak into receipts.
	safeOut := redactSecrets(string(out), secrets)

	result := map[string]any{
		"command":  cap.Backend.Command,
		"args":     args,
		"cwd":      cwd,
		"stdout":   safeOut,
		"stderr":   "",
		"payload":  safeOut,
		"exitCode": 0,
	}
	if len(cap.Backend.Credentials) > 0 {
		result["credentialsUsed"] = credentialSources(cap.Backend.Credentials)
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

// runRemote forwards a capability invocation to a clix daemon via Unix socket or HTTP.
// The capability's backend.url field specifies the daemon address:
//   - "unix:///path/to/sock" or "/path/to/sock" — Unix socket
//   - "http://host:port" or "host:port" — HTTP
//
// If backend.url is empty, CLIX_SOCKET env var is consulted.
func runRemote(cap CapabilityManifest, input map[string]any) (map[string]any, error) {
	addr := cap.Backend.URL
	if addr == "" {
		addr = daemonSocket("")
	}
	if addr == "" {
		return nil, fmt.Errorf("remote backend: no daemon address configured (set backend.url or CLIX_SOCKET)")
	}
	params := map[string]any{
		"name":      cap.Name,
		"arguments": input,
	}
	isSocket := strings.HasPrefix(addr, "unix://") || (!strings.HasPrefix(addr, "http://") && !strings.HasPrefix(addr, "https://"))
	if isSocket {
		path := strings.TrimPrefix(addr, "unix://")
		result, err := callDaemonSocket(path, "tools/call", params)
		if err != nil {
			return nil, err
		}
		return result, nil
	}
	result, err := callDaemonHTTP(addr, "tools/call", params)
	if err != nil {
		return nil, err
	}
	return result, nil
}
