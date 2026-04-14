# clix × Cursor — integration guide

Cursor has native MCP support. `clix serve` is a fully compliant MCP server.

---

## Setup (30 seconds)

```sh
# Run from your project root
clix init --cursor
```

This writes `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "clix": {
      "command": "clix",
      "args": ["serve"],
      "description": "clix — policy-enforced, sandboxed CLI gateway"
    }
  }
}
```

Restart Cursor or go to **Settings → MCP** and click **Reload** to pick up the new server.

---

## What Cursor sees

clix's `tools/list` defaults to **namespace stubs** — a small set of grouped entries rather than a flat list of every capability. This minimises the tool list registered into Cursor's context.

When Cursor's agent needs to call a tool, it drills into the namespace first, gets the full capability list, then calls `tools/call`. The namespace→capability→call flow is all handled by Cursor automatically.

---

## Manual setup (global, not project-scoped)

If you want clix available in all Cursor projects, edit `~/.cursor/mcp.json` (global MCP config):

```json
{
  "mcpServers": {
    "clix": {
      "command": "clix",
      "args": ["serve"]
    }
  }
}
```

---

## Profiles and approval mode

Cursor's agent runs through the same policy layer as every other clix client. Set the active profile before opening Cursor:

```sh
clix profile activate readonly   # Cursor agent gets read-only access
clix profile activate deploy     # Cursor agent gets deploy access
```

Capabilities with `approval_policy: require` will block until a human approves in `clix receipts list`.

---

## Troubleshooting

**MCP server not appearing in Cursor:**
- Run `clix doctor --json` to verify clix is healthy
- Make sure `clix` is on PATH (run `which clix`)
- Check Cursor's MCP log in Settings → MCP

**Tool calls failing:**
- Run `clix receipts list --status denied` to see policy denials
- Run `clix capabilities list` to confirm the pack is installed
- Check `clix status` for active profile and sandbox state
