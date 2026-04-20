# clix × Claude — integration guide

> **The recommended pattern is Pattern 1: direct CLI.** The agent calls `clix run <capability> --json`
> as a plain shell command — no tool registration, no MCP, zero context overhead.
> Patterns 2 and 3 are available for specialized use cases but are not the primary design.

Three integration patterns, ordered by how much context budget you want to spend.

---

## Pattern 1 — Direct CLI (zero context cost, recommended)

The agent runs `clix` CLI commands directly. No tool registration needed. Works anywhere Claude can execute shell commands (Claude Code, subprocess-based agents).

Paste into your system prompt or CLAUDE.md:

```
clix is a sandboxed CLI gateway. Run these directly:
  clix capabilities list --json        # browse tools
  clix capabilities search <q> --json  # search by keyword
  clix capabilities show <name> --json # get input schema for one tool
  clix run <name> -i key=val --json    # execute → {ok, result, receipt_id}
  clix run <name> --dry-run --json     # policy preview, no execution
  clix doctor --json                   # health
Exit codes: 0 ok · 1 denied · 2 needs approval
```

See [agent-quickstart.md](agent-quickstart.md) for the full reference.

---

## Pattern 2 — Two-tool API (recommended for Claude API users)

Register two meta-tools: `clix_discover` and `clix_run`. The agent discovers capabilities on demand — you pay ~400 tokens upfront regardless of how many capabilities are installed.

### Get the tool definitions

```sh
clix tools export --format two-tool
```

Paste the output JSON into your `tools` array.

### Handle tool_use responses

```python
import subprocess, json, anthropic

client = anthropic.Anthropic()

def get_clix_tools():
    result = subprocess.run(
        ["clix", "tools", "export", "--format", "two-tool"],
        capture_output=True, text=True
    )
    return json.loads(result.stdout)

def handle_tool_use(tool_name: str, tool_input: dict) -> str:
    if tool_name == "clix_discover":
        if "capability" in tool_input:
            cmd = ["clix", "capabilities", "show", tool_input["capability"], "--json"]
        elif "namespace" in tool_input:
            # list capabilities in namespace via MCP one-shot
            cmd = ["clix", "mcp", "call", "tools/list",
                   "--params", json.dumps({"namespace": tool_input["namespace"]})]
        elif "query" in tool_input:
            cmd = ["clix", "capabilities", "search", tool_input["query"], "--json"]
        else:
            cmd = ["clix", "capabilities", "list", "--json"]

    elif tool_name == "clix_run":
        cap = tool_input["capability"]
        inputs = tool_input.get("inputs", {})
        dry_run = tool_input.get("dry_run", False)
        cmd = ["clix", "run", cap, "--json"]
        if dry_run:
            cmd.append("--dry-run")
        for k, v in inputs.items():
            cmd += ["-i", f"{k}={v}"]

    result = subprocess.run(cmd, capture_output=True, text=True)
    return result.stdout or result.stderr

def run_agent(user_message: str):
    tools = get_clix_tools()
    messages = [{"role": "user", "content": user_message}]

    while True:
        response = client.messages.create(
            model="claude-opus-4-6",
            max_tokens=4096,
            tools=tools,
            messages=messages,
        )

        if response.stop_reason == "end_turn":
            return next(b.text for b in response.content if b.type == "text")

        # Process tool_use blocks
        tool_results = []
        for block in response.content:
            if block.type == "tool_use":
                result = handle_tool_use(block.name, block.input)
                tool_results.append({
                    "type": "tool_result",
                    "tool_use_id": block.id,
                    "content": result,
                })

        messages.append({"role": "assistant", "content": response.content})
        messages.append({"role": "user", "content": tool_results})
```

### TypeScript version

```typescript
import Anthropic from "@anthropic-ai/sdk";
import { execSync } from "child_process";

const client = new Anthropic();

function getClixTools() {
  const output = execSync("clix tools export --format two-tool").toString();
  return JSON.parse(output);
}

function handleToolUse(name: string, input: Record<string, unknown>): string {
  let cmd: string[];

  if (name === "clix_discover") {
    if ("capability" in input) {
      cmd = ["clix", "capabilities", "show", input.capability as string, "--json"];
    } else if ("namespace" in input) {
      cmd = ["clix", "mcp", "call", "tools/list",
             "--params", JSON.stringify({ namespace: input.namespace })];
    } else if ("query" in input) {
      cmd = ["clix", "capabilities", "search", input.query as string, "--json"];
    } else {
      cmd = ["clix", "capabilities", "list", "--json"];
    }
  } else {
    // clix_run
    const cap = input.capability as string;
    const inputs = (input.inputs as Record<string, string>) ?? {};
    const dryRun = input.dry_run as boolean ?? false;
    cmd = ["clix", "run", cap, "--json"];
    if (dryRun) cmd.push("--dry-run");
    for (const [k, v] of Object.entries(inputs)) {
      cmd.push("-i", `${k}=${v}`);
    }
  }

  try {
    return execSync(cmd.join(" ")).toString();
  } catch (e: unknown) {
    return (e as { stdout?: Buffer }).stdout?.toString() ?? String(e);
  }
}

async function runAgent(userMessage: string): Promise<string> {
  const tools = getClixTools();
  const messages: Anthropic.MessageParam[] = [{ role: "user", content: userMessage }];

  while (true) {
    const response = await client.messages.create({
      model: "claude-opus-4-6",
      max_tokens: 4096,
      tools,
      messages,
    });

    if (response.stop_reason === "end_turn") {
      const textBlock = response.content.find((b) => b.type === "text");
      return textBlock?.type === "text" ? textBlock.text : "";
    }

    const toolResults: Anthropic.ToolResultBlockParam[] = [];
    for (const block of response.content) {
      if (block.type === "tool_use") {
        toolResults.push({
          type: "tool_result",
          tool_use_id: block.id,
          content: handleToolUse(block.name, block.input as Record<string, unknown>),
        });
      }
    }

    messages.push({ role: "assistant", content: response.content });
    messages.push({ role: "user", content: toolResults });
  }
}
```

---

## Pattern 3 — Full capability registration (domain-scoped)

When you know the agent's task domain upfront (e.g. "this agent only uses kubectl"), register just that namespace's capabilities as individual Claude tools. Richer per-tool descriptions, no discovery overhead.

```sh
# Export a namespace as Claude tools
clix tools export --format claude --namespace kubectl

# Or export everything (only practical for small catalogues)
clix tools export --format claude --all
```

The output is a ready-to-use `tools` array. Pipe the tool name back to `clix run`:

```python
def handle_tool_use(tool_name: str, tool_input: dict) -> str:
    # Convert double-underscore back to dot-separated capability name
    cap_name = tool_name.replace("__", ".")
    cmd = ["clix", "run", cap_name, "--json"]
    for k, v in tool_input.items():
        cmd += ["-i", f"{k}={v}"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    return result.stdout
```

---

## Claude Code (editor integration)

```sh
# Set up clix as a project-level MCP server (run from project root)
clix init --claude-code
```

This writes:
- `.mcp.json` — Claude Code reads this to start `clix serve` as an MCP tool server
- `CLAUDE.md` — appended with the clix direct-CLI reference so Claude Code knows both interfaces

Restart Claude Code or reload MCP servers to pick up the new config.

### Namespace drill-in for MCP

Claude Code registers all MCP tools upfront. clix's `tools/list` defaults to **namespace stubs** to keep that list small:

```
tools/list {}              → [{name:"git", type:"namespace", count:4}, ...]
tools/list {namespace:"git"} → [{name:"git__status", description:...}, ...]
tools/list {all:true}      → flat list of every capability
```

This means Claude Code sees a small set of namespace tools initially, then drills in on demand — context-efficient by default.

---

## Receipt audit

Every `clix run` execution writes a receipt regardless of which integration pattern you use:

```sh
clix receipts list --json
clix receipts list --status denied
```

Receipts include: capability name, inputs, policy decision, outcome, isolation tier, binary SHA-256.
