# Agent Tool Registry — Progressive Context Loading

## The Problem

A flat `tools/list` response at scale is toxic for agent efficiency. An agent working on a
Vertex AI task doesn't need kubectl, gcloud storage, or gcloud compute in its context window.
Loading 500 tool descriptors wastes ~15,000 tokens per call and degrades tool selection quality
because the model has too much noise to reason through.

## Design: Three-Layer Progressive Loading

The solution is profile-scoped packs + hierarchical namespaces + parameterised `tools/list`.

### Layer 1 — Profile activation (coarse gate)

A profile declares which packs are active. The MCP server only exposes tools for capabilities
belonging to active packs. An agent bootstrapped with a `gcloud-aiplatform` profile never sees
kubectl or gh tools.

```
clix profile activate gcloud-aiplatform
→ tools/list now returns ~20 tools, not 500
```

This is the first filter and the cheapest. Profiles are composable — stack
`gcloud-aiplatform + kubectl-observe` for a broader agent context.

### Layer 2 — Namespace stub view (default `tools/list`)

Within the active set, `tools/list` with no parameters returns **namespace stubs** — one entry
per capability group, not one per leaf tool. The grouping algorithm uses the first two
dot-segments of the capability name:

| Capability name | Group key |
|---|---|
| `system.date` | `system` |
| `gcloud.list-projects` | `gcloud` |
| `kubectl.get-pods` | `kubectl` |
| `gcloud.aiplatform.models.list` | `gcloud.aiplatform` |
| `gcloud.aiplatform.endpoints.describe` | `gcloud.aiplatform` |

Response shape:

```json
{
  "tools": [
    { "name": "gcloud.aiplatform", "type": "namespace", "count": 6,
      "description": "Vertex AI / AI Platform — models, endpoints, jobs, datasets" },
    { "name": "gcloud",            "type": "namespace", "count": 1,
      "description": "Google Cloud Platform — projects" },
    { "name": "system",            "type": "namespace", "count": 2,
      "description": "Built-in system utilities" }
  ]
}
```

The agent sees 3–10 groups instead of hundreds of tools. It identifies the relevant namespace
and drills in with a second call.

### Layer 3 — Drill-in by namespace

```json
tools/list { "namespace": "gcloud.aiplatform" }
```

Returns the actual MCP tool descriptors (name, description, inputSchema) for every capability
whose name starts with `gcloud.aiplatform.`. The agent now has exactly the tools it needs with
full schemas, ready to call.

```json
tools/list { "all": true }
```

Returns the full flat list for backward compatibility and tooling that wants everything.

## Naming Convention

Capability names use dot-separated segments in order of specificity:

```
{cli}.{service}.{resource}.{action}
```

Examples:
- `gcloud.aiplatform.models.list`
- `gcloud.aiplatform.endpoints.describe`
- `kubectl.pods.list`
- `gh.repos.list`
- `system.date`

**Rules:**
- All lowercase, hyphens allowed within a segment
- Leaf segment is always the action verb (`list`, `describe`, `create`, `delete`, `run`)
- Read-only actions: `list`, `describe`, `get`, `logs`
- Mutating actions: `create`, `update`, `delete`, `deploy`, `run`
- Never abbreviate the service/resource segments — agents use these for discovery

## Pack Structure

Each pack declares a namespace and groups related capabilities under it:

```
packs/gcloud-aiplatform/
  pack.yaml                                  ← name, version, namespace, description
  capabilities/
    gcloud.aiplatform.models.list.yaml
    gcloud.aiplatform.models.describe.yaml
    gcloud.aiplatform.endpoints.list.yaml
    gcloud.aiplatform.endpoints.describe.yaml
    gcloud.aiplatform.jobs.list.yaml
    gcloud.aiplatform.datasets.list.yaml
  profiles/
    gcloud-aiplatform.yaml                   ← activates this pack's capabilities
```

The pack's `namespace` field in `pack.yaml` is the common prefix for all its capabilities.
This is how the stub view knows what description to surface for the group.

## Efficiency at Scale

| Scenario | Flat dump | Hierarchical |
|---|---|---|
| 500 tools, agent needs 3 | ~15K tokens | ~500 tokens |
| Agent tool selection accuracy | Degrades with noise | Stays high — scoped context |
| New pack adds 50 tools | Degrades all agents | Isolated to that namespace |
| Human `clix capabilities list` | Unusable at 500 | `--namespace gcloud.aiplatform` works |
| Cold-start agent bootstrap | Full context consumed | Namespace stubs only |

## MCP Protocol Notes

The `tools/list` namespace parameter is a clix extension — standard MCP clients receive the
stub list and can call `tools/call` on a namespace name to receive an error that explains
drill-in is needed. MCP-aware agents should check `capabilities.extensions.clix.namespaces`
in the `initialize` response to know drill-in is supported.

The `initialize` response advertises:

```json
{
  "capabilities": {
    "tools": { "listChanged": false },
    "extensions": {
      "clix": {
        "namespaces": true,
        "workflows": true,
        "onboard": true
      }
    }
  }
}
```

## `clix` CLI Parity

All namespace features surface in the CLI too:

```sh
clix capabilities list                          # stub view — namespace groups
clix capabilities list --namespace gcloud.aiplatform   # drill into namespace
clix capabilities list --all                    # flat dump
clix capabilities list --json                   # machine-readable, any of the above
```
