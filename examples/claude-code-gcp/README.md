# claude-code-gcp example

Zero to "Claude can query GCP but can't delete anything" in under 10 minutes.

## What this shows

- Claude Code calls `clix run gcloud.projects.list --json` directly as a CLI tool
- clix enforces policy: only the three read-only capabilities are allowed
- Everything else is denied by default (no allow rule = denied)
- On Linux: gcloud runs inside a jail with no network access to your host
- On macOS: same policy enforcement, no OS jail (banner printed at startup)

## Prerequisites

- clix installed (`curl -fsSL ... | sh`, see root README)
- `gcloud` CLI on PATH and authenticated (`gcloud auth application-default login`)
- Claude Code with this project's `CLAUDE.md` in the working directory

## Setup (5 minutes)

**1. Install clix**
```sh
curl -fsSL https://raw.githubusercontent.com/RyanMerlin/clix/main/scripts/install.sh | sh
clix init
```

**2. Install this example's pack**
```sh
clix pack install ./pack
```

**3. Copy the policy**
```sh
cp clix-policy.yaml ~/.clix/policy.yaml
```

**4. Start the broker** (Linux only — mints short-lived gcloud tokens)
```sh
clix broker start
# or for persistent daemon:
# clix broker install-unit && systemctl --user start clix-broker
```

**5. Verify**
```sh
clix doctor --json
# → "sandbox": "enforced" (Linux) or "policy-only" (macOS)

clix capabilities list --json
# → shows gcloud.projects.list, gcloud.compute.instances.list, gcloud.run.services.list

clix run gcloud.projects.list --json
# → JSON list of your GCP projects
```

## How Claude uses it

With `CLAUDE.md` in your working directory, Claude Code reads the tool instructions.
Claude calls clix like any CLI tool:

```
clix run gcloud.projects.list --json
clix run gcloud.compute.instances.list --json
clix receipts list --json   ← audit trail
```

Claude cannot call `gcloud` directly — the broker holds the credentials.
Claude cannot call any gcloud subcommand not in the pack — the policy denies it.

## Try to break it

Ask Claude: "Delete the project named foo" — the only delete-capable commands aren't in
the pack, so the response will be a clix `Denied` receipt. The receipt is written to the
local database and visible via `clix receipts list --json`.

## Extending

To add more capabilities:

1. Add a new entry to `pack/pack.yaml`
2. Add an allow rule to `clix-policy.yaml`
3. Run `clix pack install ./pack` again
4. Update `CLAUDE.md` to tell Claude the new capability exists

Keep mutations at `risk: high` so they go through `require_approval` unless you
explicitly add an allow rule.
