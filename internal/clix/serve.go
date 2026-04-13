package clix

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"

	"github.com/spf13/cobra"
)

func newServeCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "serve",
		Short: "Run a JSON-RPC gateway (stdin/stdout, Unix socket, or HTTP)",
		RunE: func(cmd *cobra.Command, args []string) error {
			socket, _ := cmd.Flags().GetString("socket")
			httpAddr, _ := cmd.Flags().GetString("http")
			switch {
			case socket != "":
				return serveSocket(socket)
			case httpAddr != "":
				return serveHTTP(httpAddr)
			default:
				return serveStream(os.Stdin, os.Stdout)
			}
		},
	}
	cmd.Flags().String("socket", "", "Unix socket path to listen on (e.g. /var/run/clix.sock). Also reads CLIX_SOCKET env var.")
	cmd.Flags().String("http", "", "TCP address to listen on for HTTP (e.g. :8080)")
	return cmd
}

// serveStream handles the original newline-delimited JSON-RPC over an io.Reader/Writer pair.
func serveStream(in io.Reader, out io.Writer) error {
	state, err := loadOrSeed()
	if err != nil {
		return err
	}
	registry, err := buildRegistry(state)
	if err != nil {
		return err
	}
	workflows, err := buildWorkflowRegistry(state)
	if err != nil {
		return err
	}
	return dispatchRPC(state, registry, workflows, in, out)
}

// serveSocket listens on a Unix socket. Each accepted connection is handled in its own goroutine.
func serveSocket(path string) error {
	// Remove a stale socket file if present.
	_ = os.Remove(path)
	ln, err := net.Listen("unix", path)
	if err != nil {
		return fmt.Errorf("clix: socket listen %s: %w", path, err)
	}
	defer func() {
		ln.Close()
		os.Remove(path)
	}()
	fmt.Fprintf(os.Stderr, "clix daemon listening on %s\n", path)

	state, err := loadOrSeed()
	if err != nil {
		return err
	}
	registry, err := buildRegistry(state)
	if err != nil {
		return err
	}
	workflows, err := buildWorkflowRegistry(state)
	if err != nil {
		return err
	}

	for {
		conn, err := ln.Accept()
		if err != nil {
			return err
		}
		go func(c net.Conn) {
			defer c.Close()
			_ = dispatchRPC(state, registry, workflows, c, c)
		}(conn)
	}
}

// serveHTTP listens on a TCP address and handles single JSON-RPC requests over HTTP POST.
func serveHTTP(addr string) error {
	state, err := loadOrSeed()
	if err != nil {
		return err
	}
	registry, err := buildRegistry(state)
	if err != nil {
		return err
	}
	workflows, err := buildWorkflowRegistry(state)
	if err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "clix daemon listening on http://%s\n", addr)
	mux := http.NewServeMux()
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_ = dispatchRPC(state, registry, workflows, r.Body, w)
	})
	return http.ListenAndServe(addr, mux)
}

// dispatchRPC is the core newline-delimited JSON-RPC loop shared by all transports.
func dispatchRPC(state *State, registry *CapabilityRegistry, workflows *WorkflowRegistry, in io.Reader, out io.Writer) error {
	scanner := bufio.NewScanner(in)
	enc := json.NewEncoder(out)
	for scanner.Scan() {
		line := scanner.Bytes()
		if len(line) == 0 {
			continue
		}
		var req map[string]any
		if err := json.Unmarshal(line, &req); err != nil {
			_ = enc.Encode(map[string]any{"jsonrpc": "2.0", "id": nil, "error": map[string]any{"code": -32700, "message": err.Error()}})
			continue
		}
		id := req["id"]
		method := toString(req["method"])
		params, _ := req["params"].(map[string]any)
		var result any
		switch method {
		case "initialize":
			result = map[string]any{"serverInfo": map[string]any{"name": "clix", "version": "0.1.0"}, "capabilities": map[string]any{"tools": true, "workflows": true}}
		case "tools/list":
			var tools []map[string]any
			for _, cap := range registry.All() {
				tools = append(tools, map[string]any{"name": cap.Name, "description": cap.Description, "inputSchema": cap.InputSchema})
			}
			result = map[string]any{"tools": tools}
		case "tools/call":
			outcome, err := runCapability(state, registry, state.Policy, toString(params["name"]), mustMap(params["arguments"]), ctxFromState(state), "interactive")
			if err != nil {
				_ = enc.Encode(map[string]any{"jsonrpc": "2.0", "id": id, "error": map[string]any{"code": -32000, "message": err.Error()}})
				continue
			}
			result = outcome
		case "workflows/list":
			var list []map[string]any
			for _, wf := range workflows.All() {
				list = append(list, map[string]any{"name": wf.Name, "description": wf.Description})
			}
			result = map[string]any{"workflows": list}
		case "workflows/run":
			outcome, err := runWorkflow(state, registry, workflows, state.Policy, toString(params["name"]), mustMap(params["arguments"]), ctxFromState(state), "interactive")
			if err != nil {
				_ = enc.Encode(map[string]any{"jsonrpc": "2.0", "id": id, "error": map[string]any{"code": -32000, "message": err.Error()}})
				continue
			}
			result = outcome
		default:
			_ = enc.Encode(map[string]any{"jsonrpc": "2.0", "id": id, "error": map[string]any{"code": -32601, "message": fmt.Sprintf("method not found: %s", method)}})
			continue
		}
		_ = enc.Encode(map[string]any{"jsonrpc": "2.0", "id": id, "result": result})
	}
	return scanner.Err()
}

func mustMap(v any) map[string]any {
	if m, ok := v.(map[string]any); ok {
		return m
	}
	return map[string]any{}
}
