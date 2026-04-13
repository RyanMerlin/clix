// Package clix — embedded Infisical client.
//
// This is a deliberate, minimal extraction of the production-hardened logic from
// github.com/infisical/go-sdk, retaining only Universal Auth + secret retrieval.
//
// What was kept from the SDK:
//   - Token lifecycle: expiry check, renewal vs re-auth decision, background goroutine
//   - Request-time refresh hook (safety net for GC pauses / goroutine timing misses)
//   - Simple in-memory cache (sha256-keyed, TTL-based)
//   - Concurrency guards (sync.RWMutex on token state, sync.Mutex on refresh)
//
// What was dropped:
//   - go-resty (replaced with net/http)
//   - hashicorp/golang-lru (replaced with map + sync.RWMutex + manual TTL)
//   - rs/zerolog (no logging needed at this layer)
//   - All cloud-provider auth methods (AWS IAM, GCP, OCI, K8s, Azure, LDAP)
//
// Infisical API surface used:
//   POST /v1/auth/universal-auth/login   — initial auth
//   POST /v1/auth/token/renew            — token renewal
//   GET  /v3/secrets/raw/{key}           — single secret retrieval
package clix

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sync"
	"time"
)

const (
	infisicalDefaultSiteURL = "https://app.infisical.com"
	// tokenRenewalBuffer is how many seconds before expiry we proactively refresh.
	// Mirrors the SDK's renewalBufferSeconds constant.
	tokenRenewalBuffer = 5
)

// infisicalTokenState holds the current access token and its timing metadata.
// Mirrors the SDK's MachineIdentityCredential + lastFetchedTime/firstFetchedTime fields.
type infisicalTokenState struct {
	accessToken       string
	expiresIn         int64     // seconds until expiry
	accessTokenMaxTTL int64     // seconds, absolute maximum lifetime
	lastFetchedAt     time.Time // when this token was last fetched/renewed
	firstFetchedAt    time.Time // when the original auth session started (for max TTL tracking)
}

// infisicalCache is a simple in-memory TTL cache with a sha256-keyed map.
// The SDK uses hashicorp/golang-lru; we use a plain map with expiry timestamps.
type infisicalCache struct {
	mu      sync.RWMutex
	entries map[string]infisicalCacheEntry
	ttl     time.Duration
}

type infisicalCacheEntry struct {
	value     any
	expiresAt time.Time
}

func newInfisicalCache(ttl time.Duration) *infisicalCache {
	return &infisicalCache{
		entries: make(map[string]infisicalCacheEntry),
		ttl:     ttl,
	}
}

func (c *infisicalCache) cacheKey(v any, feature string) string {
	b, _ := json.Marshal(v)
	sum := sha256.Sum256(b)
	return feature + "-" + hex.EncodeToString(sum[:])
}

func (c *infisicalCache) get(key string) (any, bool) {
	c.mu.RLock()
	e, ok := c.entries[key]
	c.mu.RUnlock()
	if !ok || time.Now().After(e.expiresAt) {
		return nil, false
	}
	return e.value, true
}

func (c *infisicalCache) set(key string, v any) {
	c.mu.Lock()
	c.entries[key] = infisicalCacheEntry{value: v, expiresAt: time.Now().Add(c.ttl)}
	c.mu.Unlock()
}

// infisicalClient is the embedded Infisical client.
type infisicalClient struct {
	siteURL      string
	clientID     string
	clientSecret string
	httpClient   *http.Client

	mu        sync.RWMutex
	token     infisicalTokenState
	refreshMu sync.Mutex // prevents concurrent refresh attempts

	cache *infisicalCache
	ctx   context.Context
}

// --- Wire types mirroring SDK API shapes ---

type universalAuthLoginRequest struct {
	ClientID     string `json:"clientId"`
	ClientSecret string `json:"clientSecret"`
}

type universalAuthLoginResponse struct {
	AccessToken       string `json:"accessToken"`
	ExpiresIn         int64  `json:"expiresIn"`
	AccessTokenMaxTTL int64  `json:"accessTokenMaxTTL"`
	TokenType         string `json:"tokenType"`
}

type tokenRenewRequest struct {
	AccessToken string `json:"accessToken"`
}

type tokenRenewResponse struct {
	AccessToken       string `json:"accessToken"`
	ExpiresIn         int64  `json:"expiresIn"`
	AccessTokenMaxTTL int64  `json:"accessTokenMaxTTL"`
	TokenType         string `json:"tokenType"`
}

type retrieveSecretResponse struct {
	Secret struct {
		SecretKey   string `json:"secretKey"`
		SecretValue string `json:"secretValue"`
	} `json:"secret"`
}

// --- Client construction ---

// newInfisicalClient creates a client and authenticates via Universal Auth.
// It starts a background token-refresh goroutine tied to ctx.
func newInfisicalClient(ctx context.Context, cfg InfisicalConfig) (*infisicalClient, error) {
	siteURL := cfg.SiteURL
	if siteURL == "" {
		siteURL = infisicalDefaultSiteURL
	}
	c := &infisicalClient{
		siteURL:      siteURL,
		clientID:     cfg.ClientID,
		clientSecret: cfg.ClientSecret,
		httpClient:   &http.Client{Timeout: 15 * time.Second},
		cache:        newInfisicalCache(30 * time.Second),
		ctx:          ctx,
	}
	if err := c.authenticate(); err != nil {
		return nil, err
	}
	go c.handleTokenLifecycle()
	return c, nil
}

// --- Authentication ---

// authenticate performs a full Universal Auth login and stores the token.
// Called on startup and as the fallback when renewal fails.
func (c *infisicalClient) authenticate() error {
	resp, err := c.callUniversalAuthLogin()
	if err != nil {
		return err
	}
	now := time.Now()
	c.mu.Lock()
	c.token = infisicalTokenState{
		accessToken:       resp.AccessToken,
		expiresIn:         resp.ExpiresIn,
		accessTokenMaxTTL: resp.AccessTokenMaxTTL,
		lastFetchedAt:     now,
		firstFetchedAt:    now,
	}
	c.mu.Unlock()
	return nil
}

// reauthenticate re-auths and preserves the original firstFetchedAt for max TTL tracking.
// Mirrors the SDK's re-auth path that resets the max TTL clock.
func (c *infisicalClient) reauthenticate() error {
	resp, err := c.callUniversalAuthLogin()
	if err != nil {
		return err
	}
	now := time.Now()
	c.mu.Lock()
	c.token = infisicalTokenState{
		accessToken:       resp.AccessToken,
		expiresIn:         resp.ExpiresIn,
		accessTokenMaxTTL: resp.AccessTokenMaxTTL,
		lastFetchedAt:     now,
		firstFetchedAt:    now, // reset: new session
	}
	c.mu.Unlock()
	return nil
}

// renewToken attempts to extend the current token's lifetime without a full re-auth.
// Falls back to reauthenticate() if renewal fails or max TTL is nearly exhausted.
// Mirrors the SDK's refreshTokenSynchronously() logic.
func (c *infisicalClient) renewToken() error {
	c.refreshMu.Lock()
	defer c.refreshMu.Unlock()

	c.mu.RLock()
	tok := c.token
	c.mu.RUnlock()

	// If the remaining max TTL is less than the current token TTL, a renewal won't
	// buy a full window — fall back to a full re-auth instead.
	// This mirrors the SDK's decision at client_auth_helper.go:62-67.
	timeSinceFirst := time.Since(tok.firstFetchedAt).Seconds()
	remainingMaxTTL := float64(tok.accessTokenMaxTTL) - timeSinceFirst
	if remainingMaxTTL < float64(tok.expiresIn) {
		return c.reauthenticate()
	}

	resp, err := c.callTokenRenew(tok.accessToken)
	if err != nil {
		// Renewal failed — fall back to full re-auth.
		return c.reauthenticate()
	}
	now := time.Now()
	c.mu.Lock()
	c.token = infisicalTokenState{
		accessToken:       resp.AccessToken,
		expiresIn:         resp.ExpiresIn,
		accessTokenMaxTTL: resp.AccessTokenMaxTTL,
		lastFetchedAt:     now,
		firstFetchedAt:    tok.firstFetchedAt, // preserved: same session
	}
	c.mu.Unlock()
	return nil
}

// isExpiringSoon returns true if the token will expire within tokenRenewalBuffer seconds.
// Mirrors the SDK's isTokenExpiringSoon() check.
func (c *infisicalClient) isExpiringSoon() bool {
	c.mu.RLock()
	tok := c.token
	c.mu.RUnlock()
	elapsed := time.Since(tok.lastFetchedAt).Seconds()
	return elapsed >= float64(tok.expiresIn-tokenRenewalBuffer)
}

// sleepDuration calculates how long to sleep before the next proactive refresh.
// Mirrors the SDK's calculateSleepTime() logic.
func (c *infisicalClient) sleepDuration() time.Duration {
	c.mu.RLock()
	tok := c.token
	c.mu.RUnlock()
	elapsed := time.Since(tok.lastFetchedAt).Seconds()
	remaining := float64(tok.expiresIn) - elapsed - tokenRenewalBuffer
	if remaining < 1 {
		remaining = 1
	}
	return time.Duration(remaining) * time.Second
}

// handleTokenLifecycle is the background goroutine that proactively refreshes the token.
// Mirrors the SDK's handleTokenLifeCycle() goroutine.
func (c *infisicalClient) handleTokenLifecycle() {
	for {
		select {
		case <-c.ctx.Done():
			return
		case <-time.After(c.sleepDuration()):
			if c.isExpiringSoon() {
				_ = c.renewToken() // best-effort; request-time hook is the safety net
			}
		}
	}
}

// accessToken returns the current access token, refreshing first if it is expiring soon.
// This is the request-time safety net — it handles cases where the background goroutine
// was delayed by GC pauses or CPU contention. Mirrors the SDK's OnBeforeRequest hook.
func (c *infisicalClient) accessToken() (string, error) {
	if c.isExpiringSoon() {
		if err := c.renewToken(); err != nil {
			return "", fmt.Errorf("infisical: token refresh failed: %w", err)
		}
	}
	c.mu.RLock()
	tok := c.token.accessToken
	c.mu.RUnlock()
	return tok, nil
}

// --- HTTP helpers ---

func (c *infisicalClient) callUniversalAuthLogin() (*universalAuthLoginResponse, error) {
	body, _ := json.Marshal(universalAuthLoginRequest{
		ClientID:     c.clientID,
		ClientSecret: c.clientSecret,
	})
	req, _ := http.NewRequestWithContext(c.ctx, http.MethodPost, c.siteURL+"/v1/auth/universal-auth/login", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	// Deliberately no Authorization header — auth endpoint must not loop.

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("infisical: universal auth login request failed: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("infisical: universal auth login returned HTTP %d", resp.StatusCode)
	}
	var out universalAuthLoginResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, fmt.Errorf("infisical: universal auth login response decode failed: %w", err)
	}
	return &out, nil
}

func (c *infisicalClient) callTokenRenew(accessToken string) (*tokenRenewResponse, error) {
	body, _ := json.Marshal(tokenRenewRequest{AccessToken: accessToken})
	req, _ := http.NewRequestWithContext(c.ctx, http.MethodPost, c.siteURL+"/v1/auth/token/renew", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+accessToken)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("infisical: token renew request failed: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("infisical: token renew returned HTTP %d", resp.StatusCode)
	}
	var out tokenRenewResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, fmt.Errorf("infisical: token renew response decode failed: %w", err)
	}
	return &out, nil
}

// RetrieveSecretOptions mirrors the SDK's RetrieveSecretOptions for a consistent interface.
type RetrieveSecretOptions struct {
	SecretKey              string
	ProjectID              string
	Environment            string
	SecretPath             string
	ExpandSecretReferences bool
}

// RetrieveSecret fetches a single secret value. Results are cached for 30 seconds.
func (c *infisicalClient) RetrieveSecret(opts RetrieveSecretOptions) (string, error) {
	if opts.SecretPath == "" {
		opts.SecretPath = "/"
	}

	// Cache lookup before making a network call.
	cacheKey := c.cache.cacheKey(opts, "RetrieveSecret")
	if cached, ok := c.cache.get(cacheKey); ok {
		if s, ok := cached.(string); ok {
			return s, nil
		}
	}

	tok, err := c.accessToken()
	if err != nil {
		return "", err
	}

	params := url.Values{}
	params.Set("environment", opts.Environment)
	params.Set("secretPath", opts.SecretPath)
	params.Set("expandSecretReferences", boolStr(opts.ExpandSecretReferences))
	if opts.ProjectID != "" {
		params.Set("workspaceId", opts.ProjectID)
	}

	endpoint := fmt.Sprintf("%s/v3/secrets/raw/%s?%s", c.siteURL, url.PathEscape(opts.SecretKey), params.Encode())
	req, _ := http.NewRequestWithContext(c.ctx, http.MethodGet, endpoint, nil)
	req.Header.Set("Authorization", "Bearer "+tok)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return "", fmt.Errorf("infisical: retrieve secret %q request failed: %w", opts.SecretKey, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNotFound {
		return "", fmt.Errorf("infisical: secret %q not found (project=%s env=%s path=%s)", opts.SecretKey, opts.ProjectID, opts.Environment, opts.SecretPath)
	}
	if resp.StatusCode != http.StatusOK {
		b, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("infisical: retrieve secret %q returned HTTP %d: %s", opts.SecretKey, resp.StatusCode, string(b))
	}
	var out retrieveSecretResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return "", fmt.Errorf("infisical: retrieve secret %q response decode failed: %w", opts.SecretKey, err)
	}

	c.cache.set(cacheKey, out.Secret.SecretValue)
	return out.Secret.SecretValue, nil
}

func boolStr(b bool) string {
	if b {
		return "true"
	}
	return "false"
}
