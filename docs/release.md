# Release

The project uses GitHub Actions to build tagged releases.

## Trigger

- Push a tag that matches `v*`
- Or start the `release` workflow manually

## Artifacts

The release pipeline builds:

- Linux `amd64`
- Linux `arm64`
- macOS `amd64`
- macOS `arm64`
- Windows `amd64`

The binaries are stamped with:

- semantic version from the tag
- commit SHA
- build timestamp

Release assets are uploaded to the GitHub release as platform-specific binaries named like `clix-linux-amd64`, `clix-darwin-arm64`, and `clix-windows-amd64.exe`.
Each binary is published with a matching `.sha256` sidecar file. The install script verifies the checksum before moving the binary into place.

The installer script downloads either:

- `https://github.com/RyanMerlin/clix/releases/latest/download/<asset>` when `CLIX_VERSION` is unset
- `https://github.com/RyanMerlin/clix/releases/download/<tag>/<asset>` when `CLIX_VERSION` is set to a tag like `v0.1.0`

For hardened installs, pin both the script URL and `CLIX_VERSION` to the same release tag.

## Local build

```powershell
go test ./...
go build ./cmd/clix
```

To embed release metadata locally:

```powershell
go build -trimpath -ldflags "-X github.com/RyanMerlin/clix/internal/clix.Version=1.0.0 -X github.com/RyanMerlin/clix/internal/clix.Commit=local -X github.com/RyanMerlin/clix/internal/clix.BuildDate=now" ./cmd/clix
```
