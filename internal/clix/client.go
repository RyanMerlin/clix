package clix

import (
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"strings"
)

// daemonSocket returns the socket path from a flag value or the CLIX_SOCKET env var.
func daemonSocket(flagVal string) string {
	if flagVal != "" {
		return flagVal
	}
	return os.Getenv("CLIX_SOCKET")
}

// callDaemonSocket sends a single JSON-RPC request over a Unix socket and returns the result.
func callDaemonSocket(socketPath, method string, params map[string]any) (map[string]any, error) {
	conn, err := net.Dial("unix", socketPath)
	if err != nil {
		return nil, fmt.Errorf("clix: cannot connect to daemon at %s: %w", socketPath, err)
	}
	defer conn.Close()

	req := map[string]any{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  method,
		"params":  params,
	}
	enc := json.NewEncoder(conn)
	if err := enc.Encode(req); err != nil {
		return nil, fmt.Errorf("clix: daemon write error: %w", err)
	}

	var rpcResp map[string]any
	if err := json.NewDecoder(conn).Decode(&rpcResp); err != nil {
		return nil, fmt.Errorf("clix: daemon response decode error: %w", err)
	}
	return extractRPCResult(rpcResp)
}

// callDaemonHTTP sends a single JSON-RPC request to an HTTP daemon endpoint.
func callDaemonHTTP(addr, method string, params map[string]any) (map[string]any, error) {
	url := addr
	if !strings.HasPrefix(url, "http://") && !strings.HasPrefix(url, "https://") {
		url = "http://" + addr
	}
	body, err := json.Marshal(map[string]any{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  method,
		"params":  params,
	})
	if err != nil {
		return nil, err
	}
	resp, err := http.Post(url, "application/json", strings.NewReader(string(body)))
	if err != nil {
		return nil, fmt.Errorf("clix: HTTP daemon call failed: %w", err)
	}
	defer resp.Body.Close()
	var rpcResp map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&rpcResp); err != nil {
		return nil, fmt.Errorf("clix: daemon response decode error: %w", err)
	}
	return extractRPCResult(rpcResp)
}

func extractRPCResult(rpcResp map[string]any) (map[string]any, error) {
	if errField, ok := rpcResp["error"]; ok {
		if errMap, ok := errField.(map[string]any); ok {
			msg, _ := errMap["message"].(string)
			return nil, fmt.Errorf("clix daemon error: %s", msg)
		}
		return nil, fmt.Errorf("clix daemon error: %v", errField)
	}
	if result, ok := rpcResp["result"].(map[string]any); ok {
		return result, nil
	}
	return map[string]any{}, nil
}
