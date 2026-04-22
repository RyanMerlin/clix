# clix — agent quickstart

> **clix is a plain CLI tool. Call it with `clix run <capability> --json`.
> There is no MCP server, no tool registration, no handshake. Just shell commands.**

Paste this into your system prompt or agent context. It's the complete reference for interacting with clix as a CLI.

---

## What clix does

clix is a CLI gateway that makes CLIs first-class tools for agents. It provides rolling discovery (browse namespaces → drill in → call), typed inputs, consistent JSON output, and a full receipts log. On Linux, subprocess calls run inside an OS-level jail. Every execution is logged regardless of isolation tier.

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

## Binding secrets from Infisical to a profile

1. Open the TUI: `clix tui`
2. Press `1` to go to the **Profiles** screen
3. Select a profile with `↑`/`↓`, then press `s` to open **Edit Secrets**
4. Press `a` to add a new binding row, type the env-var name, press Enter
5. Press `i` (or `p`) to open the Infisical tree picker — navigate folders with `↑`/`↓`/`→`/`←`, press Enter to select a secret path
6. Press `ctrl-s` to save; the secret is now injected under that env-var name whenever this profile is active
