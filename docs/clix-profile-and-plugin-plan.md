# clix Profile and Plugin Plan

## Summary

`clix` should evolve into a local-first governed CLI runtime with profiles as the main unit of composition.

The goal is to make capability packs modular, plugin-ready, and safe to stack. A profile can bundle one or more external CLIs, policy rules, playbooks, validators, and runtime settings. The user selects one or more active profiles, and `clix` resolves them into a single execution plan before any command runs.

MCP should not be the primary surface. The primary contract is the local `clix` binary plus profile/playbook manifests loaded from disk. If we need an adapter later, it can be added as one backend, not the system boundary.

## Core Design

- `clix` owns routing, policy, approval, sandbox selection, receipts, and workflow execution.
- External CLIs are treated as capabilities, not arbitrary shell strings.
- Profiles are named bundles of capabilities and rules.
- Playbooks are executable workflows inside profiles.
- Plugins are optional extension backends, not the default programming model.

## Profile Model

A profile is the unit a user installs, composes, and activates.

Recommended profile contents:

- capability definitions
- command allowlists and argument constraints
- policy defaults and approval thresholds
- workflow/playbook definitions
- sandbox profile bindings
- validators and preconditions
- optional remote backend definitions
- optional dependencies on other profiles

Example profile families:

- `base` for shared defaults and org policy
- `gcloud-readonly-planning` for safe read-only Google Cloud inspection
- `gcloud-vertex-ai-operator` for Vertex AI-specific actions
- `k8s-observe` for read-only cluster inspection
- `k8s-change-controlled` for guarded change operations
- `gh`, `git`, `infisical`, `incus`, `argocd`, `kubectl` as CLI-centric bundles

## Active Profile Resolution

`clix` should support one or more active profiles at invocation time.

Recommended resolution order:

- explicit command flags
- invocation-supplied profiles
- persisted user profile selection
- workspace or project profile
- org or system base profile
- built-in defaults

Merge rules:

- additive merge for capability lists, playbooks, validators, and read-only metadata
- deterministic override for scalar settings such as selected backend or default approval mode
- hard safety rules cannot be silently weakened by overlays
- prod network, secret, and destructive-operation policy cannot be overridden by a lower-trust profile

Profile stacking examples:

- `base + gcloud-readonly-planning`
- `base + gcloud-vertex-ai-operator`
- `base + k8s-observe + workspace-overlay`

The merged result should be inspectable before execution so the user can see exactly which capabilities and policies are active.

## Plugin-Ready Capability Packs

Profiles should be manifest-driven and versioned so they can be loaded from disk without changing the core binary.

Each capability pack should declare:

- external CLI binary name
- supported subcommands and flags
- input schema
- output schema
- validators
- sandbox profile
- approval policy
- rollback or recovery hint
- linked playbooks

This allows `clix` to add new CLIs or new operational modes by dropping in a new profile package rather than editing core code.

Recommended plugin boundaries:

- built-ins for auth, policy, receipts, profile resolution, and other high-trust core behavior
- subprocess plugins for external CLIs and heavier integrations
- optional RPC plugins for advanced integrations where process isolation is useful

## Agent Integration Without MCP as the Primary Surface

`clix` should be usable directly as a CLI surface by both humans and agents.

Preferred integration model:

- the agent invokes `clix` locally
- `clix` loads profiles, playbooks, and capabilities from disk
- commands stay governed inside `clix`
- no agent-side registration step is required for basic use

If a future adapter is needed, MCP can be implemented as an optional compatibility layer. It should not own the core control plane or be required for day-to-day use.

Suggested terminology:

- `profile` for a composable capability bundle
- `playbook` for a workflow inside a profile
- `plugin` for an extension backend
- `skill` for a curated agent-facing wrapper around profiles or playbooks

## CLI Surface

The CLI should make profile intent visible and explicit.

Recommended commands:

- `clix profile list`
- `clix profile describe <name>`
- `clix profile use <name[,name...]>`
- `clix profile active`
- `clix profile merge --profile a --profile b`
- `clix capability list`
- `clix capability describe <name>`
- `clix playbook list`
- `clix playbook run <name>`
- `clix run <capability>`
- `clix doctor`
- `clix receipts list`

Useful inspection commands:

- show merged profile state before execution
- show policy decisions per capability
- show selected sandbox and approval mode
- show the provenance of each active rule or capability

## Implementation Phases

1. Add profile manifests and profile loading.
2. Add profile merge and precedence rules.
3. Split existing seeded capabilities into reusable profile packs.
4. Add explicit capability pack metadata for external CLIs such as `kubectl`, `gcloud`, `gh`, `git`, `infisical`, `incus`, and `argocd`.
5. Add playbook binding inside profiles.
6. Add install/discovery for local plugin packs.
7. Add optional compatibility adapters only after the native CLI/profile model is stable.

## Acceptance Criteria

- A user can activate one or more profiles and see the exact merged result.
- A profile can define a new external CLI capability without changing core code.
- Two profiles can compose without ambiguous behavior.
- Safety rules remain stronger than overlays.
- Agents can use `clix` through the local binary without MCP registration.
- The single binary remains the authoritative policy and execution boundary.

## Assumptions

- Local-first is the default deployment mode.
- MCP is optional and secondary.
- Profiles are versioned and manifest-driven.
- Plugin support should preserve the single-binary distribution story.
- The first operational profiles should prioritize the CLIs already called out by the user: `kubectl`, `gcloud`, `infisical`, `gh`, `git`, `incus`, and `argocd`.
