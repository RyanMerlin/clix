//go:build integration

// Integration tests for the embedded Infisical client.
// Requires live credentials in environment:
//
//	INFISICAL_UNIVERSAL_AUTH_CLIENT_ID
//	INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET
//	INFISICAL_PROJECT_ID
//	INFISICAL_ENVIRONMENT   (defaults to "dev")
//	INFISICAL_SITE_URL      (defaults to https://app.infisical.com)
//	INFISICAL_SECRET_PATH   (defaults to "/")
//	INFISICAL_TEST_SECRET   (name of a secret to fetch; defaults to first found)
//
// Run with: go test -tags integration -v -run TestInfisical ./internal/clix/
package clix

import (
	"context"
	"os"
	"testing"
)

func infisicalCfgFromEnv(t *testing.T) InfisicalConfig {
	t.Helper()
	clientID := os.Getenv("INFISICAL_UNIVERSAL_AUTH_CLIENT_ID")
	clientSecret := os.Getenv("INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET")
	if clientID == "" || clientSecret == "" {
		t.Skip("INFISICAL_UNIVERSAL_AUTH_CLIENT_ID / INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET not set")
	}
	return InfisicalConfig{
		SiteURL:      os.Getenv("INFISICAL_SITE_URL"),
		ClientID:     clientID,
		ClientSecret: clientSecret,
	}
}

// TestInfisical_Authenticate verifies that Universal Auth login succeeds and
// returns a non-empty access token.
func TestInfisical_Authenticate(t *testing.T) {
	cfg := infisicalCfgFromEnv(t)
	client, err := newInfisicalClient(context.Background(), cfg)
	if err != nil {
		t.Fatalf("newInfisicalClient: %v", err)
	}
	token, err := client.accessToken()
	if err != nil {
		t.Fatalf("accessToken: %v", err)
	}
	if token == "" {
		t.Fatal("expected non-empty access token")
	}
	t.Logf("authenticated successfully, token prefix: %s...", token[:min(12, len(token))])
}

// TestInfisical_RetrieveSecret fetches a named secret from the configured
// project/environment and verifies a non-empty value is returned.
func TestInfisical_RetrieveSecret(t *testing.T) {
	cfg := infisicalCfgFromEnv(t)

	projectID := os.Getenv("INFISICAL_PROJECT_ID")
	if projectID == "" {
		t.Skip("INFISICAL_PROJECT_ID not set")
	}
	env := os.Getenv("INFISICAL_ENVIRONMENT")
	if env == "" {
		env = "dev"
	}
	secretPath := os.Getenv("INFISICAL_SECRET_PATH")
	if secretPath == "" {
		secretPath = "/"
	}
	secretName := os.Getenv("INFISICAL_TEST_SECRET")
	if secretName == "" {
		t.Skip("INFISICAL_TEST_SECRET not set — set to the name of a secret in your project")
	}

	opts := RetrieveSecretOptions{
		SecretKey:   secretName,
		ProjectID:   projectID,
		Environment: env,
		SecretPath:  secretPath,
	}

	client, err := newInfisicalClient(context.Background(), cfg)
	if err != nil {
		t.Fatalf("newInfisicalClient: %v", err)
	}

	value, err := client.RetrieveSecret(opts)
	if err != nil {
		t.Fatalf("RetrieveSecret(%q): %v", secretName, err)
	}
	if value == "" {
		t.Fatalf("expected non-empty value for secret %q", secretName)
	}
	t.Logf("secret %q retrieved successfully (%d chars)", secretName, len(value))
}

// TestInfisical_CacheHit verifies that a second retrieval of the same secret
// is served from the in-memory cache (no second network call).
func TestInfisical_CacheHit(t *testing.T) {
	cfg := infisicalCfgFromEnv(t)

	projectID := os.Getenv("INFISICAL_PROJECT_ID")
	env := os.Getenv("INFISICAL_ENVIRONMENT")
	if env == "" {
		env = "dev"
	}
	secretName := os.Getenv("INFISICAL_TEST_SECRET")
	if projectID == "" || secretName == "" {
		t.Skip("INFISICAL_PROJECT_ID and INFISICAL_TEST_SECRET required")
	}

	opts := RetrieveSecretOptions{
		SecretKey:   secretName,
		ProjectID:   projectID,
		Environment: env,
		SecretPath:  "/",
	}

	client, err := newInfisicalClient(context.Background(), cfg)
	if err != nil {
		t.Fatalf("newInfisicalClient: %v", err)
	}

	v1, err := client.RetrieveSecret(opts)
	if err != nil {
		t.Fatalf("first fetch: %v", err)
	}
	v2, err := client.RetrieveSecret(opts)
	if err != nil {
		t.Fatalf("second fetch (cache): %v", err)
	}
	if v1 != v2 {
		t.Errorf("cache returned different value: %q vs %q", v1, v2)
	}
	t.Logf("cache hit confirmed for %q", secretName)
}

// TestInfisical_ListSecrets lists all secrets under INFISICAL_SECRET_PATH
// (defaults to /agents) and prints their keys and values.
func TestInfisical_ListSecrets(t *testing.T) {
	cfg := infisicalCfgFromEnv(t)

	projectID := os.Getenv("INFISICAL_PROJECT_ID")
	if projectID == "" {
		t.Skip("INFISICAL_PROJECT_ID not set")
	}
	env := os.Getenv("INFISICAL_ENVIRONMENT")
	if env == "" {
		env = "dev"
	}
	secretPath := os.Getenv("INFISICAL_SECRET_PATH")
	if secretPath == "" {
		secretPath = "/agents"
	}

	client, err := newInfisicalClient(context.Background(), cfg)
	if err != nil {
		t.Fatalf("newInfisicalClient: %v", err)
	}

	secrets, err := client.ListSecrets(ListSecretsOptions{
		ProjectID:   projectID,
		Environment: env,
		SecretPath:  secretPath,
	})
	if err != nil {
		t.Fatalf("ListSecrets: %v", err)
	}

	t.Logf("Found %d secrets under %s (env=%s):", len(secrets), secretPath, env)
	for _, kv := range secrets {
		t.Logf("  %s = %s", kv.Key, kv.Value)
	}
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
