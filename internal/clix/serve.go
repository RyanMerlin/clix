package clix

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"
)

func newServeCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "serve",
		Short: "Run a local JSON-RPC bridge",
		RunE: func(cmd *cobra.Command, args []string) error {
			return serve(os.Stdin, os.Stdout)
		},
	}
}

func serve(in io.Reader, out io.Writer) error {
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
