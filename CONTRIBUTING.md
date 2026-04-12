# Contributing

## Local development

- Install Go 1.26+
- Run `go test ./...`
- Run `go build ./cmd/clix`

## Design rules

- Keep the core binary local-first.
- Add new external CLI support through profiles and capability manifests.
- Preserve policy checks and receipts for every execution path.
