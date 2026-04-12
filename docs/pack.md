# Packs

`clix` packs are local directory bundles that group profiles, capabilities, workflows, and optional plugins.

## Layout

```text
my-pack/
  pack.json
  profiles/
  capabilities/
  workflows/
  plugins/
```

## Commands

- `clix pack discover <path>`: inspect a source directory without installing it
- `clix pack install <path>`: copy a pack into the local packs directory
- `clix pack bundle <path>`: create a distributable bundle archive
- `clix pack publish <path>`: publish a bundle into a local registry directory
- `clix pack scaffold <name>`: generate a new pack from a preset template
- `clix pack onboard <name>`: probe a CLI and generate a first-pass pack scaffold
- `clix pack list`: list installed packs
- `clix pack show <name>`: show a single installed pack

## Behavior

- Installed packs live under `~/.clix/packs` unless `CLIX_HOME` changes the base directory.
- Pack manifests are discovered from `pack.json`.
- Profiles, capabilities, and workflows inside installed packs are automatically discovered by `clix`.
- The core binary keeps policy and receipts as the enforcement boundary; packs only add capabilities and workflow bundles.

## Built-in Packs

The seeded install includes packs for:

- `gcloud-readonly-planning`
- `gcloud-vertex-ai-operator`
- `kubectl-observe`
- `kubectl-change-controlled`
- `gh-readonly`
- `git-observer`
- `infisical-readonly`
- `incus-readonly`
- `argocd-observe`

Each pack ships a profile, real command-backed capabilities, and at least one workflow playbook.

## Authoring Workflow

Recommended workflow for a new pack:

1. Run `clix pack scaffold my-pack --preset read-only` for a known pattern, or `clix pack onboard my-pack --command mycli` for an unknown CLI.
2. Edit `pack.json` to describe the pack.
3. Fill in `profiles/`, `capabilities/`, and `workflows/`.
4. Run `clix pack discover ./my-pack` to validate the manifest.
5. Run `clix pack bundle ./my-pack` to create a distributable archive.
6. Run `clix pack install ./my-pack-v1.clixpack.zip` to install it locally.

The scaffold is intentionally minimal so authors can start simple and add only the pieces they need.
When you supply `--command`, the generated pack binds the scaffold to that external CLI and records the binding in the profile settings and README.
`clix pack onboard` runs a help/version probe sequence first, then chooses the closest preset and writes an `onboard.json` report with the observed sections and commands.
`clix pack publish` copies the bundle archive into `~/.clix/bundles/published` by default and writes a small `index.json` for discovery.

## Presets

- `read-only`: safe inspection packs with a single version/info/help capability
- `change-controlled`: a `plan` + `apply` shape with approval for the mutating step
- `operator`: status, reconcile, and verify with an explicit approval gate on reconcile

Use `--preset` to choose the starting point that matches the intended trust level.

## Onboarding Unknown CLIs

`clix pack onboard` is the fastest way to bring a new CLI into the model:

1. Run it against the CLI binary or a container image that contains the CLI.
2. It probes `--help`, `help`, `--version`, `version`, and `info` style entry points.
3. It infers the closest preset from the observed output.
4. It generates a scaffold with the CLI bound into the capability backend.

Example:

```powershell
clix pack onboard my-tool --command mytool --runner local
clix pack onboard my-tool --command mytool --runner docker --image ghcr.io/acme/mytool:latest
```

The output includes a JSON report of the probe attempts so authors can inspect what the onboarding pass discovered.

## Bundles

Bundles are zip archives containing the pack tree plus a `bundle.json` manifest and a `.sha256` sidecar.

Use bundles when you want to move packs between machines or share them with teammates without copying directories by hand.
