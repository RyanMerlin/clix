# Packs

`clix` packs are local directory bundles that group profiles, capabilities, and workflows.

## Layout

```text
my-pack/
  pack.yaml
  profiles/
  capabilities/
  workflows/
```

## Commands

- `clix pack discover <path>`: inspect a source directory without installing it
- `clix pack install <path>`: copy a pack into the local packs directory
- `clix pack validate <path>`: validate pack manifests against the schema
- `clix pack bundle <path>`: create a distributable `.clixpack` archive (optionally signed)
- `clix pack diff <installed> <new_path>`: diff a local pack against an installed version
- `clix pack publish <path>`: publish a bundle into a local registry directory
- `clix pack scaffold <name>`: generate a new pack from a preset template
- `clix pack onboard <name> --command <cli>`: probe a CLI and generate a first-pass scaffold
- `clix pack list`: list installed packs
- `clix pack show <name>`: show a single installed pack

## Behavior

- Installed packs live under `~/.clix/packs` (override with `CLIX_HOME`).
- Pack manifests are discovered from `pack.yaml`.
- Profiles, capabilities, and workflows inside installed packs are automatically loaded by `clix`.
- Policy and receipts are the enforcement boundary; packs only add capabilities and workflows.

## Built-in Packs

These packs ship with clix and install via `clix init`:

| Pack | Capabilities | Use case |
|------|-------------|---------|
| `base` | `system.date`, `system.echo` | Sanity checks, builtins |
| `gcloud-readonly` | list-projects, compute-instances, container-clusters, storage-buckets, iam-service-accounts, functions | GCP read-only inspection |
| `kubectl-observe` | get-pods, get-nodes, get-namespaces, get-services, get-deployments, logs, get-events, describe-pod | Kubernetes observation |
| `gh-readonly` | list-repos, pr-list, issue-list, pr-view, workflow-list | GitHub read-only |
| `git-readonly` | status, log, diff, branch-list | Local git inspection |
| `docker-observe` | ps, images, logs, inspect, stats, network-list | Docker read-only |
| `podman-observe` | ps, images, logs, inspect, pod-list | Podman read-only |
| `aws-readonly` | whoami, ec2-instances, s3-buckets, ecs-clusters, lambda-list, ecr-repos | AWS read-only |
| `az-readonly` | account-list, group-list, vm-list, aks-list, storage-accounts, acr-list | Azure read-only |
| `helm-observe` | list, status, get-values, history | Helm release inspection |
| `gcloud-aiplatform` | datasets-list, endpoints-list, endpoints-describe, jobs-list, models-list, models-describe | Vertex AI inspection |

All read-only packs use `sideEffectClass: readOnly` and `risk: low`. They cannot write, delete, or modify any resource.

## Authoring Workflow

1. `clix pack scaffold my-pack --preset read-only` — or `clix pack onboard my-pack --command mycli` for an unknown CLI.
2. Edit `pack.yaml`, fill in `profiles/` and `capabilities/`.
3. `clix pack validate ./my-pack` — validate the manifest.
4. `clix pack bundle ./my-pack` — create a distributable archive (add `--sign` to sign with your key).
5. `clix pack install ./my-pack.clixpack` — install locally.

## Presets

| Preset | Shape | Approval |
|--------|-------|---------|
| `read-only` | Single list/inspect capability | None |
| `change-controlled` | `plan` + `apply` pair | Required on `apply` |
| `operator` | status, reconcile, verify | Required on `reconcile` |

## Pack Signing

Bundles can be signed with an Ed25519 key:

```sh
clix pack keygen                        # generate ~/.clix/pack-signing.pem
clix pack bundle ./my-pack --sign       # sign on bundle
clix pack trust ./trusted-key.pub       # add a public key to the trust store
clix pack install ./my-pack.clixpack --verify-sig   # enforce signature on install
clix pack verify ./my-pack.clixpack     # verify without installing
```

## Onboarding Unknown CLIs

`clix pack onboard` is the fastest way to bring a new CLI into the model:

```sh
clix pack onboard my-tool --command mytool --json
```

It probes `--help`, `--version`, and `info` entry points, infers the closest preset,
and generates a scaffold with the CLI bound into the capability backend. The `--json`
flag outputs a structured probe report.
