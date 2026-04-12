# clix Architecture

`clix` is a policy-first CLI gateway.

Main pieces:

- `profiles`: named bundles of capabilities, workflows, and policy
- `capabilities`: typed execution units that wrap external CLIs or builtins
- `policy`: allow, deny, or approval decisions for every run
- `workflows`: composed playbooks made from capabilities
- `receipts`: append-only execution records

The current Go implementation is the foundation for:

- modular external CLI packs
- profile stacking
- plugin-ready execution backends
- optional bridge servers

The intended extension model is profile-first, not MCP-first.
