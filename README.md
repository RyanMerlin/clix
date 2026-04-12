# clix

`clix` is a local-first CLI control plane for agentic tool use.

This repository is implemented in Go.

## Build

```powershell
go test ./...
go build ./cmd/clix
```

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/RyanMerlin/clix/v0.1.2/scripts/install.sh | CLIX_VERSION=v0.1.2 sh
```

The installer verifies the downloaded binary against the release checksum before installing it.

## Releases

Tagged releases are built by GitHub Actions for:

- Linux `amd64` and `arm64`
- macOS `amd64` and `arm64`
- Windows `amd64`

Releases also include checksum sidecars, an SBOM, and GitHub provenance attestation artifacts.

## Quick start

```powershell
go run .\cmd\clix init
go run .\cmd\clix capabilities list
go run .\cmd\clix pack list
go run .\cmd\clix pack onboard demo-pack --command mycli
go run .\cmd\clix profile active
go run .\cmd\clix version
go run .\cmd\clix run system.date
```

The state directory defaults to `%USERPROFILE%\.clix` on Windows and `~/.clix` elsewhere. Override with `CLIX_HOME`.

## Docs

- [Architecture](docs/architecture.md)
- [Profile and plugin plan](docs/clix-profile-and-plugin-plan.md)
- [Packs](docs/pack.md)
- [Release process](docs/release.md)
