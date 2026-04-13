package clix

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

const defaultApprovalTimeoutSeconds = 300

// ApprovalRequest is the payload POSTed to the approval webhook.
// The webhook service (Slack bot, web UI, PagerDuty, etc.) receives this,
// presents it to a human, and returns an ApprovalResponse.
type ApprovalRequest struct {
	RequestID  string         `json:"requestId"`
	Capability string         `json:"capability"`
	Input      map[string]any `json:"input"`
	Context    map[string]string `json:"context"`
	Policy     map[string]any `json:"policy"`
	Risk       string         `json:"risk,omitempty"`
	Reason     string         `json:"reason,omitempty"`
	CreatedAt  string         `json:"createdAt"`
}

// ApprovalResponse is what the webhook must return.
type ApprovalResponse struct {
	Approved bool   `json:"approved"`
	Reason   string `json:"reason,omitempty"`
	Approver string `json:"approver,omitempty"`
}

// requestApproval POSTs an ApprovalRequest to the configured webhook and waits for
// an ApprovalResponse. It is fail-safe: any error, non-200 response, or timeout
// is treated as a denial.
//
// Returns (approved bool, approver string, reason string, err error).
// err is only non-nil for programming errors (e.g. JSON marshal failure);
// network/timeout failures are returned as approved=false with a reason string.
func requestApproval(cfg ApprovalGateConfig, cap CapabilityManifest, input map[string]any, execCtx map[string]string, policy map[string]any) (bool, string, string, error) {
	timeout := cfg.TimeoutSeconds
	if timeout <= 0 {
		timeout = defaultApprovalTimeoutSeconds
	}

	reqID := newID()
	payload := ApprovalRequest{
		RequestID:  reqID,
		Capability: cap.Name,
		Input:      input,
		Context:    execCtx,
		Policy:     policy,
		Risk:       cap.Risk,
		Reason:     stringFromMap(policy, "reason"),
		CreatedAt:  currentISO(),
	}

	body, err := json.Marshal(payload)
	if err != nil {
		return false, "", "", fmt.Errorf("approval: marshal request: %w", err)
	}

	httpClient := &http.Client{Timeout: time.Duration(timeout) * time.Second}

	req, err := http.NewRequest(http.MethodPost, cfg.WebhookURL, bytes.NewReader(body))
	if err != nil {
		return false, "", "approval: failed to build request: " + err.Error(), nil
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Clix-Request-ID", reqID)
	req.Header.Set("X-Clix-Capability", cap.Name)
	for k, v := range cfg.Headers {
		req.Header.Set(k, v)
	}

	resp, err := httpClient.Do(req)
	if err != nil {
		return false, "", "approval webhook unreachable: " + err.Error(), nil
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		b, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
		return false, "", fmt.Sprintf("approval webhook returned HTTP %d: %s", resp.StatusCode, string(b)), nil
	}

	var ar ApprovalResponse
	if err := json.NewDecoder(resp.Body).Decode(&ar); err != nil {
		return false, "", "approval webhook response decode failed: " + err.Error(), nil
	}

	return ar.Approved, ar.Approver, ar.Reason, nil
}

func stringFromMap(m map[string]any, key string) string {
	if v, ok := m[key].(string); ok {
		return v
	}
	return ""
}
