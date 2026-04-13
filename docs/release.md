# Release

The project uses GitHub Actions to build tagged releases.

## Trigger

- Push a tag that matches `v*`
- Or start the `release` workflow manually

## Artifacts

The release pipeline builds:

- Linux `x86_64` and `aarch64`
- macOS `x86_64` and `aarch64`
- Windows `x86_64`

The binaries are stamped with:

- semantic version from the tag
- commit SHA
- build timestamp

Release assets are uploaded to the GitHub release as platform-specific binaries named like `clix-linux-amd64`, `clix-darwin-arm64`, and `clix-windows-amd64.exe`.
Each binary is published with a matching `.sha256` sidecar file. The install script verifies the checksum before moving the binary into place.
Each binary also ships with an SPDX SBOM (`.spdx.json`) and a matching checksum sidecar.
The release workflow also produces:

- an SBOM in SPDX JSON format
- a GitHub provenance attestation for the built release artifacts

Those artifacts live alongside the binaries in the release and give you traceability without adding any install-time complexity.

The installer script downloads either:

- `https://github.com/RyanMerlin/clix/releases/latest/download/<asset>` when `CLIX_VERSION` is unset
- `https://github.com/RyanMerlin/clix/releases/download/<tag>/<asset>` when `CLIX_VERSION` is set to a tag like `v0.1.0`

For hardened installs, pin both the script URL and `CLIX_VERSION` to the same release tag.
Set `CLIX_STRICT_VERIFY=1` to also verify the SBOM asset and GitHub attestations with `gh attestation verify`.

## Local build

```sh
cargo test
cargo build -p clix-cli
```

To produce a release binary with LTO:

```sh
cargo build -p clix-cli --release
```
