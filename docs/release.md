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

## Local build

```powershell
go test ./...
go build ./cmd/clix
```

To embed release metadata locally:

```powershell
go build -trimpath -ldflags "-X github.com/RyanMerlin/clix/internal/clix.Version=1.0.0 -X github.com/RyanMerlin/clix/internal/clix.Commit=local -X github.com/RyanMerlin/clix/internal/clix.BuildDate=now" ./cmd/clix
```
