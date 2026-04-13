package clix

import (
	"context"
	"fmt"
	"os"
	"strings"
	"sync"
)

// infisicalClientOnce guards lazy initialization of the shared embedded Infisical client.
var (
	infisicalClientOnce sync.Once
	sharedInfisical     *infisicalClient
	sharedInfisicalErr  error
)

// getInfisicalClient returns a lazily initialized, authenticated Infisical client.
// Authentication uses Universal Auth. Credentials are resolved in order:
//  1. cfg.ClientID / cfg.ClientSecret (from clix config.json)
//  2. INFISICAL_UNIVERSAL_AUTH_CLIENT_ID / INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET env vars
func getInfisicalClient(cfg InfisicalConfig) (*infisicalClient, error) {
	infisicalClientOnce.Do(func() {
		clientID := cfg.ClientID
		if clientID == "" {
			clientID = os.Getenv("INFISICAL_UNIVERSAL_AUTH_CLIENT_ID")
		}
		clientSecret := cfg.ClientSecret
		if clientSecret == "" {
			clientSecret = os.Getenv("INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET")
		}
		if clientID == "" || clientSecret == "" {
			sharedInfisicalErr = fmt.Errorf(
				"infisical: no credentials configured — set clientId/clientSecret in config.json or " +
					"INFISICAL_UNIVERSAL_AUTH_CLIENT_ID / INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET env vars",
			)
			return
		}
		resolved := InfisicalConfig{
			SiteURL:      cfg.SiteURL,
			ClientID:     clientID,
			ClientSecret: clientSecret,
		}
		sharedInfisical, sharedInfisicalErr = newInfisicalClient(context.Background(), resolved)
	})
	return sharedInfisical, sharedInfisicalErr
}

// fetchInfisicalSecret retrieves a single secret value using the embedded client.
func fetchInfisicalSecret(cfg InfisicalConfig, ref InfisicalRef) (string, error) {
	client, err := getInfisicalClient(cfg)
	if err != nil {
		return "", err
	}
	return client.RetrieveSecret(RetrieveSecretOptions{
		SecretKey:              ref.SecretName,
		ProjectID:              ref.ProjectID,
		Environment:            ref.Environment,
		SecretPath:             ref.SecretPath,
		ExpandSecretReferences: true,
	})
}

// resolveCredentials resolves a list of CredentialSources into a map of envVar → value.
func resolveCredentials(sources []CredentialSource, cfg InfisicalConfig) (map[string]string, error) {
	out := make(map[string]string, len(sources))
	for _, src := range sources {
		if src.InjectAs == "" {
			return nil, fmt.Errorf("credential source has empty injectAs field")
		}
		var value string
		switch src.Type {
		case "env":
			if src.EnvVar == "" {
				return nil, fmt.Errorf("credential %q: type=env requires envVar", src.InjectAs)
			}
			value = os.Getenv(src.EnvVar)
			if value == "" {
				return nil, fmt.Errorf("credential %q: env var %q is not set", src.InjectAs, src.EnvVar)
			}
		case "literal":
			if src.Value == "" {
				return nil, fmt.Errorf("credential %q: type=literal requires value", src.InjectAs)
			}
			value = src.Value
		case "infisical":
			if src.Infisical == nil {
				return nil, fmt.Errorf("credential %q: type=infisical requires infisical reference", src.InjectAs)
			}
			var err error
			value, err = fetchInfisicalSecret(cfg, *src.Infisical)
			if err != nil {
				return nil, err
			}
		default:
			return nil, fmt.Errorf("credential %q: unknown source type %q (must be env, literal, or infisical)", src.InjectAs, src.Type)
		}
		out[src.InjectAs] = value
	}
	return out, nil
}

// buildSubprocessEnv returns an env slice for exec.Cmd.Env.
// It starts with the current process environment and overlays injected credentials,
// replacing any existing value for the same key rather than appending a duplicate.
func buildSubprocessEnv(injected map[string]string) []string {
	base := os.Environ()
	if len(injected) == 0 {
		return base
	}
	// Normalize override keys for case-insensitive dedup on the outgoing slice.
	override := make(map[string]string, len(injected))
	for k, v := range injected {
		override[strings.ToUpper(k)] = v
	}
	out := make([]string, 0, len(base)+len(injected))
	for _, kv := range base {
		key := strings.ToUpper(strings.SplitN(kv, "=", 2)[0])
		if _, replacing := override[key]; !replacing {
			out = append(out, kv)
		}
	}
	for k, v := range injected {
		out = append(out, k+"="+v)
	}
	return out
}

// redactSecrets replaces any credential value in s with "[REDACTED]".
// Called before logging subprocess output into receipts.
func redactSecrets(s string, secrets map[string]string) string {
	for _, v := range secrets {
		if v != "" {
			s = strings.ReplaceAll(s, v, "[REDACTED]")
		}
	}
	return s
}

// credentialSources returns a safe-to-log summary of credential sources (no values).
func credentialSources(sources []CredentialSource) []map[string]string {
	out := make([]map[string]string, 0, len(sources))
	for _, src := range sources {
		entry := map[string]string{"injectAs": src.InjectAs, "type": src.Type}
		switch src.Type {
		case "env":
			entry["envVar"] = src.EnvVar
		case "infisical":
			if src.Infisical != nil {
				entry["secretName"] = src.Infisical.SecretName
				entry["projectId"] = src.Infisical.ProjectID
				entry["environment"] = src.Infisical.Environment
			}
		}
		out = append(out, entry)
	}
	return out
}
