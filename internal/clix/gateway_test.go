package clix

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

// ── Credential resolution ────────────────────────────────────────────────────

func TestResolveCredentials_Env(t *testing.T) {
	t.Setenv("TEST_TOKEN", "secret-value")
	sources := []CredentialSource{
		{InjectAs: "MY_TOKEN", Type: "env", EnvVar: "TEST_TOKEN"},
	}
	got, err := resolveCredentials(sources, InfisicalConfig{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got["MY_TOKEN"] != "secret-value" {
		t.Fatalf("expected secret-value, got %q", got["MY_TOKEN"])
	}
}

func TestResolveCredentials_Literal(t *testing.T) {
	sources := []CredentialSource{
		{InjectAs: "API_KEY", Type: "literal", Value: "hardcoded"},
	}
	got, err := resolveCredentials(sources, InfisicalConfig{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got["API_KEY"] != "hardcoded" {
		t.Fatalf("expected hardcoded, got %q", got["API_KEY"])
	}
}

func TestResolveCredentials_MissingEnvVar(t *testing.T) {
	os.Unsetenv("DEFINITELY_NOT_SET_XYZ")
	sources := []CredentialSource{
		{InjectAs: "X", Type: "env", EnvVar: "DEFINITELY_NOT_SET_XYZ"},
	}
	_, err := resolveCredentials(sources, InfisicalConfig{})
	if err == nil {
		t.Fatal("expected error for missing env var")
	}
}

func TestResolveCredentials_UnknownType(t *testing.T) {
	sources := []CredentialSource{
		{InjectAs: "X", Type: "vault"},
	}
	_, err := resolveCredentials(sources, InfisicalConfig{})
	if err == nil {
		t.Fatal("expected error for unknown source type")
	}
}

func TestResolveCredentials_EmptyInjectAs(t *testing.T) {
	sources := []CredentialSource{
		{InjectAs: "", Type: "literal", Value: "v"},
	}
	_, err := resolveCredentials(sources, InfisicalConfig{})
	if err == nil {
		t.Fatal("expected error for empty injectAs")
	}
}

// ── buildSubprocessEnv ───────────────────────────────────────────────────────

func TestBuildSubprocessEnv_NoInjection(t *testing.T) {
	env := buildSubprocessEnv(nil)
	if len(env) == 0 {
		t.Fatal("expected at least some inherited env vars")
	}
}

func TestBuildSubprocessEnv_InjectAndOverride(t *testing.T) {
	t.Setenv("EXISTING_KEY", "old-value")
	injected := map[string]string{
		"EXISTING_KEY": "new-value",
		"NEW_KEY":      "added",
	}
	env := buildSubprocessEnv(injected)

	found := map[string]string{}
	for _, kv := range env {
		parts := strings.SplitN(kv, "=", 2)
		if len(parts) == 2 {
			found[parts[0]] = parts[1]
		}
	}
	if found["EXISTING_KEY"] != "new-value" {
		t.Errorf("EXISTING_KEY should be overridden, got %q", found["EXISTING_KEY"])
	}
	if found["NEW_KEY"] != "added" {
		t.Errorf("NEW_KEY should be added, got %q", found["NEW_KEY"])
	}
	// Verify no duplicate entries for the overridden key.
	count := 0
	for _, kv := range env {
		if strings.HasPrefix(kv, "EXISTING_KEY=") {
			count++
		}
	}
	if count != 1 {
		t.Errorf("EXISTING_KEY appears %d times, expected exactly 1", count)
	}
}

// ── redactSecrets ────────────────────────────────────────────────────────────

func TestRedactSecrets_Replaces(t *testing.T) {
	secrets := map[string]string{"TOKEN": "super-secret"}
	out := redactSecrets("output: super-secret value", secrets)
	if strings.Contains(out, "super-secret") {
		t.Errorf("credential value leaked: %q", out)
	}
	if !strings.Contains(out, "[REDACTED]") {
		t.Errorf("expected [REDACTED] in output: %q", out)
	}
}

func TestRedactSecrets_EmptyValueSafe(t *testing.T) {
	secrets := map[string]string{"EMPTY": ""}
	out := redactSecrets("hello world", secrets)
	if out != "hello world" {
		t.Errorf("unexpected output: %q", out)
	}
}

// ── Infisical client (mock HTTP server) ──────────────────────────────────────

func newMockInfisicalServer(t *testing.T, secretValue string) (server *httptest.Server, authCalls *atomic.Int32) {
	t.Helper()
	authCalls = new(atomic.Int32)
	mux := http.NewServeMux()

	mux.HandleFunc("/api/v1/auth/universal-auth/login", func(w http.ResponseWriter, r *http.Request) {
		authCalls.Add(1)
		json.NewEncoder(w).Encode(universalAuthLoginResponse{
			AccessToken: "test-token", ExpiresIn: 3600, AccessTokenMaxTTL: 7200, TokenType: "Bearer",
		})
	})
	mux.HandleFunc("/api/v1/auth/token/renew", func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(tokenRenewResponse{
			AccessToken: "renewed-token", ExpiresIn: 3600, AccessTokenMaxTTL: 7200, TokenType: "Bearer",
		})
	})
	mux.HandleFunc("/api/v3/secrets/raw/", func(w http.ResponseWriter, r *http.Request) {
		key := strings.TrimPrefix(r.URL.Path, "/api/v3/secrets/raw/")
		json.NewEncoder(w).Encode(map[string]any{
			"secret": map[string]any{"secretKey": key, "secretValue": secretValue},
		})
	})

	return httptest.NewServer(mux), authCalls
}

func TestInfisicalClient_AuthAndRetrieve(t *testing.T) {
	srv, authCalls := newMockInfisicalServer(t, "my-secret-value")
	defer srv.Close()

	client, err := newInfisicalClient(context.Background(), InfisicalConfig{
		SiteURL: srv.URL, ClientID: "test-id", ClientSecret: "test-secret",
	})
	if err != nil {
		t.Fatalf("client creation failed: %v", err)
	}
	val, err := client.RetrieveSecret(RetrieveSecretOptions{
		SecretKey: "MY_SECRET", ProjectID: "proj-123", Environment: "dev",
	})
	if err != nil {
		t.Fatalf("retrieve secret failed: %v", err)
	}
	if val != "my-secret-value" {
		t.Errorf("expected my-secret-value, got %q", val)
	}
	if authCalls.Load() != 1 {
		t.Errorf("expected 1 auth call, got %d", authCalls.Load())
	}
}

func TestInfisicalClient_CacheHit(t *testing.T) {
	retrieveCalls := new(atomic.Int32)
	mux := http.NewServeMux()
	mux.HandleFunc("/api/v1/auth/universal-auth/login", func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(universalAuthLoginResponse{
			AccessToken: "tok", ExpiresIn: 3600, AccessTokenMaxTTL: 7200,
		})
	})
	mux.HandleFunc("/api/v3/secrets/raw/", func(w http.ResponseWriter, r *http.Request) {
		retrieveCalls.Add(1)
		json.NewEncoder(w).Encode(map[string]any{
			"secret": map[string]any{"secretKey": "K", "secretValue": "cached-value"},
		})
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	client, err := newInfisicalClient(context.Background(), InfisicalConfig{
		SiteURL: srv.URL, ClientID: "id", ClientSecret: "secret",
	})
	if err != nil {
		t.Fatalf("client: %v", err)
	}

	opts := RetrieveSecretOptions{SecretKey: "K", ProjectID: "p", Environment: "dev"}
	for i := 0; i < 3; i++ {
		val, err := client.RetrieveSecret(opts)
		if err != nil {
			t.Fatalf("retrieve %d: %v", i, err)
		}
		if val != "cached-value" {
			t.Errorf("retrieve %d: expected cached-value, got %q", i, val)
		}
	}
	if retrieveCalls.Load() != 1 {
		t.Errorf("expected 1 HTTP call (cache should serve rest), got %d", retrieveCalls.Load())
	}
}

func TestInfisicalClient_TokenRenewalOnExpiry(t *testing.T) {
	renewCalls := new(atomic.Int32)
	mux := http.NewServeMux()
	mux.HandleFunc("/api/v1/auth/universal-auth/login", func(w http.ResponseWriter, r *http.Request) {
		// TTL of 1 second — isExpiringSoon fires immediately since buffer=5s > 1s.
		json.NewEncoder(w).Encode(universalAuthLoginResponse{
			AccessToken: "initial-token", ExpiresIn: 1, AccessTokenMaxTTL: 7200,
		})
	})
	mux.HandleFunc("/api/v1/auth/token/renew", func(w http.ResponseWriter, r *http.Request) {
		renewCalls.Add(1)
		json.NewEncoder(w).Encode(tokenRenewResponse{
			AccessToken: "renewed-token", ExpiresIn: 3600, AccessTokenMaxTTL: 7200,
		})
	})
	// Echo back the token in use so we can verify renewal happened.
	mux.HandleFunc("/api/v3/secrets/raw/", func(w http.ResponseWriter, r *http.Request) {
		tok := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
		json.NewEncoder(w).Encode(map[string]any{
			"secret": map[string]any{"secretKey": "K", "secretValue": tok},
		})
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	client, err := newInfisicalClient(context.Background(), InfisicalConfig{
		SiteURL: srv.URL, ClientID: "id", ClientSecret: "secret",
	})
	if err != nil {
		t.Fatalf("client: %v", err)
	}

	// With ExpiresIn=1 and buffer=5, the token is already "expiring soon".
	// The request-time hook in accessToken() must renew before the GET.
	val, err := client.RetrieveSecret(RetrieveSecretOptions{
		SecretKey: "K", ProjectID: "p", Environment: "dev",
	})
	if err != nil {
		t.Fatalf("retrieve: %v", err)
	}
	if val != "renewed-token" {
		t.Errorf("expected renewed-token to be in use, got %q", val)
	}
	if renewCalls.Load() < 1 {
		t.Errorf("expected at least 1 renewal call, got %d", renewCalls.Load())
	}
}

func TestInfisicalClient_BadCredentials(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "unauthorized", http.StatusUnauthorized)
	}))
	defer srv.Close()

	_, err := newInfisicalClient(context.Background(), InfisicalConfig{
		SiteURL: srv.URL, ClientID: "bad", ClientSecret: "bad",
	})
	if err == nil {
		t.Fatal("expected error for bad credentials")
	}
}

func TestInfisicalClient_SecretNotFound(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/v1/auth/universal-auth/login", func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(universalAuthLoginResponse{
			AccessToken: "tok", ExpiresIn: 3600, AccessTokenMaxTTL: 7200,
		})
	})
	mux.HandleFunc("/api/v3/secrets/raw/", func(w http.ResponseWriter, r *http.Request) {
		http.NotFound(w, r)
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	client, err := newInfisicalClient(context.Background(), InfisicalConfig{
		SiteURL: srv.URL, ClientID: "id", ClientSecret: "secret",
	})
	if err != nil {
		t.Fatalf("client: %v", err)
	}
	_, err = client.RetrieveSecret(RetrieveSecretOptions{
		SecretKey: "MISSING", ProjectID: "p", Environment: "dev",
	})
	if err == nil {
		t.Fatal("expected error for missing secret")
	}
}

// ── Unix socket daemon transport ─────────────────────────────────────────────

// startTestDaemon starts a dispatchRPC loop on a Unix socket in a goroutine.
// It returns the socket path and blocks until the listener is ready.
func startTestDaemon(t *testing.T, state *State, registry *CapabilityRegistry, workflows *WorkflowRegistry) string {
	t.Helper()
	sockPath := filepath.Join(t.TempDir(), "clix-test.sock")
	os.Remove(sockPath)

	ln, err := net.Listen("unix", sockPath)
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	t.Cleanup(func() { ln.Close(); os.Remove(sockPath) })

	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go func(c net.Conn) {
				defer c.Close()
				_ = dispatchRPC(state, registry, workflows, c, c)
			}(conn)
		}
	}()
	return sockPath
}

func TestServeSocket_ToolsList(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed: %v", err)
	}
	registry, err := buildRegistry(state)
	if err != nil {
		t.Fatalf("registry: %v", err)
	}
	workflows, err := buildWorkflowRegistry(state)
	if err != nil {
		t.Fatalf("workflows: %v", err)
	}
	sockPath := startTestDaemon(t, state, registry, workflows)

	result, err := callDaemonSocket(sockPath, "tools/list", map[string]any{})
	if err != nil {
		t.Fatalf("tools/list: %v", err)
	}
	tools, _ := result["tools"].([]any)
	if len(tools) == 0 {
		t.Errorf("expected tools in response, got %#v", result)
	}
}

func TestServeSocket_ToolsCall_Builtin(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed: %v", err)
	}
	registry, err := buildRegistry(state)
	if err != nil {
		t.Fatalf("registry: %v", err)
	}
	workflows, err := buildWorkflowRegistry(state)
	if err != nil {
		t.Fatalf("workflows: %v", err)
	}
	sockPath := startTestDaemon(t, state, registry, workflows)

	result, err := callDaemonSocket(sockPath, "tools/call", map[string]any{
		"name":      "system.date",
		"arguments": map[string]any{},
	})
	if err != nil {
		t.Fatalf("tools/call: %v", err)
	}
	if ok, _ := result["ok"].(bool); !ok {
		t.Errorf("expected ok=true, got %#v", result)
	}
}

func TestServeSocket_UnknownMethod(t *testing.T) {
	home := t.TempDir()
	state, _ := SeedState(home)
	registry, _ := buildRegistry(state)
	workflows, _ := buildWorkflowRegistry(state)
	sockPath := startTestDaemon(t, state, registry, workflows)

	// callDaemonSocket returns an error when the RPC response contains an error field.
	_, err := callDaemonSocket(sockPath, "nonexistent/method", map[string]any{})
	if err == nil {
		t.Fatal("expected error for unknown method")
	}
}

// ── Remote backend ────────────────────────────────────────────────────────────

func TestRemoteBackend_ForwardsToSocket(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed: %v", err)
	}
	registry, err := buildRegistry(state)
	if err != nil {
		t.Fatalf("registry: %v", err)
	}
	workflows, err := buildWorkflowRegistry(state)
	if err != nil {
		t.Fatalf("workflows: %v", err)
	}
	sockPath := startTestDaemon(t, state, registry, workflows)

	// A capability with type=remote pointing at the test daemon socket.
	cap := CapabilityManifest{
		Name:    "system.date",
		Version: 1,
		Backend: CapabilityBackend{Type: "remote", URL: sockPath},
	}
	result, err := runRemote(cap, map[string]any{})
	if err != nil {
		t.Fatalf("runRemote: %v", err)
	}
	if ok, _ := result["ok"].(bool); !ok {
		t.Errorf("expected ok=true from remote backend, got %#v", result)
	}
}

// ── Credential-mediated subprocess (literal source, no network) ───────────────

func TestSubprocess_CredentialInjectionAndRedaction(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed: %v", err)
	}

	// A capability that echoes env vars — we verify the secret is injected but
	// redacted from the receipt/output returned to the caller.
	cap := CapabilityManifest{
		Name:    "test.echo-env",
		Version: 1,
		Backend: CapabilityBackend{
			Type:    "subprocess",
			Command: "sh",
			Args:    []string{"-c", "echo $SECRET_VAL"},
			Credentials: []CredentialSource{
				{InjectAs: "SECRET_VAL", Type: "literal", Value: "top-secret-123"},
			},
		},
		Risk:            "low",
		SideEffectClass: "read_only",
	}

	result, err := runSubprocess(cap, map[string]string{"cwd": t.TempDir()}, []string{"-c", "echo $SECRET_VAL"}, state.Config.Infisical)
	if err != nil {
		t.Fatalf("subprocess: %v", err)
	}

	stdout, _ := result["stdout"].(string)
	if strings.Contains(stdout, "top-secret-123") {
		t.Errorf("credential value leaked into output: %q", stdout)
	}
	if !strings.Contains(stdout, "[REDACTED]") {
		t.Errorf("expected [REDACTED] in output, got: %q", stdout)
	}

	// credentialsUsed should be present but contain no values.
	used, _ := result["credentialsUsed"].([]map[string]string)
	if len(used) == 0 {
		t.Error("expected credentialsUsed in result")
	}
}

// ── Approval gate ─────────────────────────────────────────────────────────────

func TestApprovalGate_Approved(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Verify the request shape.
		var req ApprovalRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Errorf("decode approval request: %v", err)
			http.Error(w, "bad request", http.StatusBadRequest)
			return
		}
		if req.Capability == "" || req.RequestID == "" {
			t.Errorf("expected capability and requestId in payload, got %+v", req)
		}
		// Approve.
		json.NewEncoder(w).Encode(ApprovalResponse{
			Approved: true,
			Approver: "alice",
			Reason:   "looks good",
		})
	}))
	defer srv.Close()

	cfg := ApprovalGateConfig{WebhookURL: srv.URL, TimeoutSeconds: 5}
	cap := CapabilityManifest{Name: "kubectl.delete", Risk: "high"}
	approved, approver, reason, err := requestApproval(cfg, cap, map[string]any{}, map[string]string{}, map[string]any{"decision": "require_approval", "reason": "high risk"})
	if err != nil {
		t.Fatalf("requestApproval: %v", err)
	}
	if !approved {
		t.Errorf("expected approved=true")
	}
	if approver != "alice" {
		t.Errorf("expected approver=alice, got %q", approver)
	}
	if reason != "looks good" {
		t.Errorf("expected reason='looks good', got %q", reason)
	}
}

func TestApprovalGate_Denied(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(ApprovalResponse{
			Approved: false,
			Approver: "bob",
			Reason:   "too risky",
		})
	}))
	defer srv.Close()

	cfg := ApprovalGateConfig{WebhookURL: srv.URL, TimeoutSeconds: 5}
	cap := CapabilityManifest{Name: "kubectl.delete", Risk: "high"}
	approved, _, reason, err := requestApproval(cfg, cap, map[string]any{}, map[string]string{}, map[string]any{})
	if err != nil {
		t.Fatalf("requestApproval: %v", err)
	}
	if approved {
		t.Errorf("expected approved=false")
	}
	if reason != "too risky" {
		t.Errorf("expected reason='too risky', got %q", reason)
	}
}

func TestApprovalGate_WebhookTimeout(t *testing.T) {
	// Server that responds only after the client has timed out and disconnected.
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Wait for client to disconnect (timeout) rather than blocking forever,
		// so httptest.Server.Close() can drain connections cleanly.
		select {
		case <-r.Context().Done():
		case <-time.After(10 * time.Second):
		}
	}))
	defer srv.Close()

	cfg := ApprovalGateConfig{WebhookURL: srv.URL, TimeoutSeconds: 1}
	cap := CapabilityManifest{Name: "test.cap"}
	approved, _, reason, err := requestApproval(cfg, cap, map[string]any{}, map[string]string{}, map[string]any{})
	if err != nil {
		t.Fatalf("unexpected hard error: %v", err)
	}
	// Timeout must be a soft denial, not a hard error.
	if approved {
		t.Error("timed-out approval should deny, not approve")
	}
	if reason == "" {
		t.Error("expected a reason for timeout denial")
	}
}

func TestApprovalGate_Non200Response(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "internal server error", http.StatusInternalServerError)
	}))
	defer srv.Close()

	cfg := ApprovalGateConfig{WebhookURL: srv.URL, TimeoutSeconds: 5}
	cap := CapabilityManifest{Name: "test.cap"}
	approved, _, _, err := requestApproval(cfg, cap, map[string]any{}, map[string]string{}, map[string]any{})
	if err != nil {
		t.Fatalf("unexpected hard error: %v", err)
	}
	if approved {
		t.Error("non-200 webhook response should deny")
	}
}

func TestApprovalGate_CustomHeaders(t *testing.T) {
	var gotAuth string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAuth = r.Header.Get("Authorization")
		json.NewEncoder(w).Encode(ApprovalResponse{Approved: true})
	}))
	defer srv.Close()

	cfg := ApprovalGateConfig{
		WebhookURL:     srv.URL,
		TimeoutSeconds: 5,
		Headers:        map[string]string{"Authorization": "Bearer test-token"},
	}
	cap := CapabilityManifest{Name: "test.cap"}
	_, _, _, err := requestApproval(cfg, cap, map[string]any{}, map[string]string{}, map[string]any{})
	if err != nil {
		t.Fatalf("requestApproval: %v", err)
	}
	if gotAuth != "Bearer test-token" {
		t.Errorf("expected Authorization header, got %q", gotAuth)
	}
}

// TestApprovalGate_EndToEnd verifies that runCapability executes after webhook approval
// and is denied (without execution) when the webhook rejects.
func TestApprovalGate_EndToEnd_ApprovedThenDenied(t *testing.T) {
	home := t.TempDir()
	state, err := SeedState(home)
	if err != nil {
		t.Fatalf("seed: %v", err)
	}

	var webhookResponse ApprovalResponse
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(webhookResponse)
	}))
	defer srv.Close()

	state.Config.ApprovalGate = ApprovalGateConfig{WebhookURL: srv.URL, TimeoutSeconds: 5}
	// Force require_approval for system.date by overriding the policy.
	state.Policy = PolicyBundle{
		DefaultDecision: "deny",
		Rules: []PolicyRule{
			{Effect: "require_approval", Match: PolicyMatch{Capabilities: []string{"system.date"}}},
		},
	}

	registry, err := buildRegistry(state)
	if err != nil {
		t.Fatalf("registry: %v", err)
	}

	// Case 1: webhook approves — capability must execute and receipt must show approval.
	webhookResponse = ApprovalResponse{Approved: true, Approver: "alice", Reason: "ok"}
	outcome, err := runCapability(state, registry, state.Policy, "system.date", map[string]any{}, ctxFromState(state), "interactive")
	if err != nil {
		t.Fatalf("run (approved): %v", err)
	}
	if ok, _ := outcome["ok"].(bool); !ok {
		t.Errorf("approved: expected ok=true, got %#v", outcome)
	}
	receipt, _ := outcome["receipt"].(map[string]any)
	approval, _ := receipt["approval"].(map[string]any)
	if approval["approver"] != "alice" {
		t.Errorf("expected approver=alice in receipt, got %#v", approval)
	}

	// Case 2: webhook denies — capability must not execute.
	webhookResponse = ApprovalResponse{Approved: false, Approver: "bob", Reason: "denied"}
	outcome, err = runCapability(state, registry, state.Policy, "system.date", map[string]any{}, ctxFromState(state), "interactive")
	if err != nil {
		t.Fatalf("run (denied): %v", err)
	}
	if ok, _ := outcome["ok"].(bool); ok {
		t.Errorf("denied: expected ok=false, got %#v", outcome)
	}
	if outcome["approvalRequired"] == true {
		t.Error("denied: approvalRequired should be false when webhook denied")
	}
}
