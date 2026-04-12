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
- `clix pack list`: list installed packs
- `clix pack show <name>`: show a single installed pack

## Behavior

- Installed packs live under `~/.clix/packs` unless `CLIX_HOME` changes the base directory.
- Pack manifests are discovered from `pack.json`.
- Profiles, capabilities, and workflows inside installed packs are automatically discovered by `clix`.
- The core binary keeps policy and receipts as the enforcement boundary; packs only add capabilities and workflow bundles.
