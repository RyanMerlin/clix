# clix — agent quickstart

Paste this into your system prompt or agent context. It's the complete reference for interacting with clix without MCP.

---

## What clix does

clix is a sandboxed CLI gateway. It gates your CLI tools (git, kubectl, gcloud, etc.) behind policy and OS-level isolation. Every execution is logged to a receipts database.

## Core commands

```sh
# Browse available tools (lean, ~15 tokens per entry)
clix capabilities list --json

# Get full schema for one tool
clix capabilities show git.status --json

# Find tools by keyword
clix capabilities search "list projects" --json

# Run a tool (returns structured JSON)
clix run git.status --json
clix run git.log -i limit=10 --json
clix run kubectl.get-pods -i namespace=default --json

# Preview without executing (policy + isolation info, no receipt)
clix run git.status --dry-run --json

# Gateway health
clix doctor --json
```

## Return shape for `clix run --json`

```json
{
  "ok": true,
  "receipt_id": "uuid",
  "result": { "stdout": "...", "stderr": "...", "exit_code": 0 },
  "approval_required": false,
  "reason": null
}
```

## Exit codes

| Code | Meaning |
|------|---------|
| 0    | success |
| 1    | policy denied |
| 2    | needs human approval, or unknown input key |
| 77   | blocked by active profile (shims only) |
| 126  | no capability matched argv (shims only) |
| 127  | gateway unreachable (shims only) |

## Profiles (permission levels)

```sh
clix profile list --json        # see available profiles
clix profile activate readonly  # switch to read-only mode
clix profile activate write     # escalate to write access
```

## Shims (run CLIs directly)

If shims are installed, you can invoke CLIs as if they were native:

```sh
git status        # routes through clix if git shim is installed
kubectl get pods  # same
```

The gateway must be running: `clix serve --socket /tmp/clix-gateway.sock`

## One-shot MCP (for schema inspection)

```sh
clix mcp call tools/list --params '{"namespace":"git"}'
clix mcp call tools/list --params '{"all":true}'
```

No server needed — runs in-process and exits.

---

**That's it.** No MCP tool registration required. Discover on demand, run with `clix run --json`.
