# Claude Code — GCP access via clix

## HOW TO USE CLIX — READ THIS FIRST

**clix is NOT an MCP server. Call it directly as a CLI tool.**

```sh
clix run <capability-name> --json
```

That's it. No tool registration. No MCP handshake. No server. Just run the command.

---

## What you can do

```sh
clix run gcloud.projects.list --json
clix run gcloud.compute.instances.list --json
clix run gcloud.run.services.list --json
```

## What you cannot do

Anything not explicitly listed above is **denied by default**. The policy engine rejects
it before any process is spawned. You will get a `Denied` receipt, not an error.

Do NOT attempt to call `gcloud` directly — the credentials are held by the broker and
are not available in your environment.

## Discovering capabilities

```sh
clix capabilities list --json              # all available capabilities
clix capabilities show <name> --json       # input schema for one capability
clix run <name> --dry-run --json           # policy decision, no execution
```

## Auditing what ran

```sh
clix receipts list --json                  # all invocations
clix receipts list --json --status denied  # only denied calls
clix receipts show <receipt-id> --json     # full detail
```

## Health check

```sh
clix doctor --json
```

Returns sandbox mode (`enforced` on Linux, `policy-only` on macOS), broker status,
capability count, and recent receipt stats.
