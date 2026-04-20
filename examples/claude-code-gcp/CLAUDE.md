# Claude Code — GCP access via clix

This project uses **clix** to give you controlled access to Google Cloud CLI tools.

## What you can do

- List GCP projects: `clix run gcloud.projects.list --json`
- List Compute Engine instances: `clix run gcloud.compute.instances.list --json`
- View Cloud Run services: `clix run gcloud.run.services.list --json`

## What you cannot do

Anything not in the list above is denied by policy. Attempting to call a gcloud subcommand
not wrapped by a clix capability will return a `Denied` receipt.

## Running commands

```
clix run <capability-name> --json           # run with JSON output
clix run <capability-name> --dry-run --json # preview policy decision, no execution
clix capabilities list --json              # see all available capabilities
clix receipts list --json                  # audit log of all invocations
```

## Credentials

clix-broker manages your GCP credentials. Tokens are minted fresh per-invocation and
injected into the sandboxed worker process. Your `~/.config/gcloud/` files are never
directly accessible inside the sandbox.

## Policy

Default policy: **deny all, allow explicit**. Only the capabilities listed above are
permitted. To add more capabilities, update `clix-policy.yaml` and reload clix.
