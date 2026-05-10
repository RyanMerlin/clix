# clix quickstart

If you are new to clix, this is the shortest path from zero to a working setup.

## 1. Install clix

```sh
CLIX_VERSION=v0.5.2 curl -fsSL https://raw.githubusercontent.com/RyanMerlin/clix/v0.5.2/scripts/install.sh | sh
```

That installs the tagged `v0.5.2` release binary into `~/.local/bin` by default.

## 2. Initialize the local workspace

```sh
clix init
```

This seeds the built-in packs and creates the local state under `~/.clix` unless you override it with `CLIX_HOME`.

## 3. Verify the install

```sh
clix doctor --json
```

If that succeeds, clix is ready to use.

## 4. Try the first command

```sh
clix capabilities list --json
clix capabilities show git.status --json
clix run git.status --json
```

The first command shows what is installed. The second shows the schema for one capability. The third executes it and returns structured JSON.

## 5. Optional: use the TUI

```sh
clix tui
```

The TUI is useful for exploring packs, profiles, and secrets without memorizing every flag.

## 6. Optional: integrate with an editor

If you want clix inside an agent editor, pick one of these:

```sh
clix init --claude-code
clix init --cursor
```

## For agents

If you are wiring clix into an agent prompt or runtime, use [docs/agent-quickstart.md](agent-quickstart.md) for the direct-CLI reference.
