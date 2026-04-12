# clix Architecture

`clix` is a policy-first CLI gateway.

Main pieces:

- `packs`: installable bundles that package profiles, capabilities, workflows, and plugins
- `bundle artifacts`: zip-based pack distributions with checksum sidecars
- `pack onboarding`: probe unknown CLIs from help/version output and generate a first-pass scaffold
- `profiles`: named bundles of capabilities, workflows, and policy
- `capabilities`: typed execution units that wrap external CLIs or builtins
- `policy`: allow, deny, or approval decisions for every run
- `workflows`: composed playbooks made from capabilities
- `receipts`: append-only execution records

The current Go implementation is the foundation for:

- modular external CLI packs
- pack discovery and installation from local directories
- profile stacking
- plugin-ready execution backends
- optional bridge servers

Seeded packs currently cover:

- `gcloud`
- `kubectl`
- `gh`
- `git`
- `infisical`
- `incus`
- `argocd`

The intended extension model is profile-first, not MCP-first.
