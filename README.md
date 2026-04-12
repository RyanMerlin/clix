# clix

`clix` is a local-first CLI control plane for agentic tool use.

This repository is implemented in Go.

## Build

```powershell
go test ./...
go build ./cmd/clix
```

## Quick start

```powershell
go run .\cmd\clix init
go run .\cmd\clix capabilities list
go run .\cmd\clix profile active
go run .\cmd\clix run system.date
```

The state directory defaults to `%USERPROFILE%\.clix` on Windows and `~/.clix` elsewhere. Override with `CLIX_HOME`.
