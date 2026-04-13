# clix

`clix` is a local-first CLI control plane for agentic tool use.

Implemented in Rust as a Cargo workspace (`clix-core`, `clix-cli`, `clix-serve`).

## Build

```sh
cargo build -p clix-cli
```

Run tests:

```sh
cargo test
```

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/RyanMerlin/clix/main/scripts/install.sh | sh
```

The installer verifies the downloaded binary against the release checksum before installing.
Set `CLIX_STRICT_VERIFY=1` to also verify the SBOM asset and GitHub attestations when `gh` is available.

## Releases

Tagged releases are built by GitHub Actions for:

- Linux `x86_64` and `aarch64`
- macOS `x86_64` and `aarch64`
- Windows `x86_64`

Releases include checksum sidecars, an SBOM, and GitHub provenance attestation artifacts.

## Quick start

```sh
clix init
clix capabilities list
clix pack list
clix run system.date
clix version
```

All read commands support `--json` for machine-readable output:

```sh
clix capabilities list --json
clix receipts list --json
clix status --json
```

The state directory defaults to `~/.clix` (`%USERPROFILE%\.clix` on Windows). Override with `CLIX_HOME`.

## Serve (MCP / JSON-RPC)

Start an MCP-compatible JSON-RPC server:

```sh
# stdio transport (default, for agent tool use)
clix serve

# HTTP transport
clix serve --http --port 3000

# Unix socket
clix serve --socket /tmp/clix.sock
```

The server implements MCP protocol version `2024-11-05` (`tools/*`, `resources/*`) plus clix extensions (`workflows/*`, `capabilities/*`, `receipts/*`).

## Workspace layout

```
crates/
  clix-core/    # Pure library — manifests, policy, execution, receipts, secrets, packs
  clix-cli/     # Thin tokio binary — all subcommands
  clix-serve/   # tokio + axum — JSON-RPC dispatch, MCP transport
packs/
  base/         # Built-in: system.date, system.echo
  kubectl-observe/
  gcloud-readonly/
  gh-readonly/
```

## Packs

Packs bundle capabilities, workflows, and profiles as installable YAML manifests:

```sh
clix pack list
clix pack install ./my-pack.clixpack
clix pack validate ./my-pack/
clix pack diff ./my-pack/ --against installed
clix pack bundle ./my-pack/ --out my-pack.clixpack
clix pack scaffold --name my-pack --commands "mycli"
```

## Docs

- [Architecture](docs/architecture.md)
- [Packs](docs/pack.md)
- [Release process](docs/release.md)
- [Design spec](docs/superpowers/specs/2026-04-13-rust-rewrite-design.md)
