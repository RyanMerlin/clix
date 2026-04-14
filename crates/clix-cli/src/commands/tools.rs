//! `clix tools export` — emit capability definitions in AI SDK formats.
//!
//! Formats:
//!   `claude`   — Claude API `tools` array  (`{name, description, input_schema}`)
//!   `gemini`   — Gemini `function_declarations` array  (`{name, description, parameters}`)
//!   `openai`   — OpenAI `tools` array  (`{type:"function", function:{name, description, parameters}}`)
//!   `two-tool` — Minimal 2-tool pattern for any Claude/OpenAI-compatible API (recommended for large catalogues)
//!
//! Scoping flags:
//!   `--namespace NS`  — only capabilities in that namespace group
//!   `--all`           — flat list of every capability (default: namespace stub view for two-tool; full list otherwise)
use anyhow::Result;
use clix_core::loader::build_registry;
use clix_core::registry::CapabilityRegistry;
use clix_core::state::{home_dir, ClixState};
use clix_core::manifest::capability::CapabilityManifest;
use crate::output::print_json;

#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat {
    Claude,
    Gemini,
    OpenAi,
    TwoTool,
}

impl ExportFormat {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "gemini" => Ok(Self::Gemini),
            "openai" | "openai-compat" => Ok(Self::OpenAi),
            "two-tool" | "twotool" | "2tool" => Ok(Self::TwoTool),
            other => anyhow::bail!("unknown format '{other}'. Use: claude, gemini, openai, two-tool"),
        }
    }
}

pub fn export(format_str: &str, namespace: Option<&str>, all: bool) -> Result<()> {
    let format = ExportFormat::from_str(format_str)?;
    let state = ClixState::load(home_dir())?;
    let registry = build_registry(&state)?;

    match format {
        ExportFormat::TwoTool => export_two_tool(&registry),
        ExportFormat::Claude => export_claude(&registry, namespace, all),
        ExportFormat::Gemini => export_gemini(&registry, namespace, all),
        ExportFormat::OpenAi => export_openai(&registry, namespace, all),
    }
}

// ── Two-tool pattern ──────────────────────────────────────────────────────────

/// Minimal 2-tool registration for any Claude/OpenAI-compatible API.
///
/// Registers `clix_discover` and `clix_run`. The agent discovers capabilities
/// on demand; you only pay ~400 tokens upfront regardless of how many capabilities
/// are installed.
fn export_two_tool(registry: &CapabilityRegistry) -> Result<()> {
    let ns_list: Vec<String> = registry.namespaces().iter().map(|s| s.key.clone()).collect();
    let ns_hint = if ns_list.is_empty() {
        "No namespaces installed yet.".to_string()
    } else {
        format!("Available namespaces: {}.", ns_list.join(", "))
    };

    let tools = serde_json::json!([
        {
            "name": "clix_discover",
            "description": format!(
                "Browse and search clix capabilities — a sandboxed gateway for CLI tools (git, kubectl, gcloud, etc.). \
                Call with 'query' to search by keyword, 'namespace' to list capabilities in a group, \
                or 'capability' to get the full input schema for a specific capability. \
                Call with no arguments for a namespace overview. {}",
                ns_hint
            ),
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search capabilities by keyword (name or description)"
                    },
                    "namespace": {
                        "type": "string",
                        "description": "List all capabilities in this namespace group (e.g. 'git', 'gcloud')"
                    },
                    "capability": {
                        "type": "string",
                        "description": "Get the full input schema for a specific capability name"
                    }
                }
            }
        },
        {
            "name": "clix_run",
            "description": "Execute a clix capability by name. Use clix_discover first to find the right capability and see its required inputs. \
                Returns {ok, result: {stdout, stderr, exit_code}, receipt_id, approval_required}.",
            "input_schema": {
                "type": "object",
                "required": ["capability"],
                "properties": {
                    "capability": {
                        "type": "string",
                        "description": "Capability name (e.g. 'git.status', 'kubectl.get-pods')"
                    },
                    "inputs": {
                        "type": "object",
                        "description": "Input key-value pairs per the capability's input schema"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "If true, evaluate policy and return what would happen without executing"
                    }
                }
            }
        }
    ]);

    println!("{}", serde_json::to_string_pretty(&tools)?);
    eprintln!();
    eprintln!("// Two-tool pattern: register the above tools array in your API call.");
    eprintln!("// Handle tool_use responses: route 'clix_discover' to `clix capabilities` commands,");
    eprintln!("//   and 'clix_run' to `clix run <capability> --json`.");
    eprintln!("// See docs/integration-claude.md for a full Python/TypeScript implementation.");
    Ok(())
}

// ── Claude format ─────────────────────────────────────────────────────────────

fn export_claude(registry: &CapabilityRegistry, namespace: Option<&str>, all: bool) -> Result<()> {
    let caps = select_caps(registry, namespace, all);
    let tools: Vec<serde_json::Value> = caps.iter().map(|cap| claude_tool(cap)).collect();
    print_json(&tools);
    Ok(())
}

fn claude_tool(cap: &CapabilityManifest) -> serde_json::Value {
    serde_json::json!({
        "name": sanitize_tool_name(&cap.name),
        "description": cap.description.as_deref().unwrap_or(&cap.name),
        "input_schema": cap.input_schema,
    })
}

// ── Gemini format ─────────────────────────────────────────────────────────────

fn export_gemini(registry: &CapabilityRegistry, namespace: Option<&str>, all: bool) -> Result<()> {
    let caps = select_caps(registry, namespace, all);
    let decls: Vec<serde_json::Value> = caps.iter().map(|cap| gemini_decl(cap)).collect();
    let output = serde_json::json!({ "function_declarations": decls });
    print_json(&output);
    Ok(())
}

fn gemini_decl(cap: &CapabilityManifest) -> serde_json::Value {
    // Gemini uses uppercase type names ("OBJECT", "STRING", etc.)
    let params = to_gemini_schema(&cap.input_schema);
    serde_json::json!({
        "name": sanitize_tool_name(&cap.name),
        "description": cap.description.as_deref().unwrap_or(&cap.name),
        "parameters": params,
    })
}

fn to_gemini_schema(schema: &serde_json::Value) -> serde_json::Value {
    match schema {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if k == "type" {
                    // Gemini uses uppercase type names
                    if let Some(s) = v.as_str() {
                        out.insert("type".to_string(), serde_json::Value::String(s.to_uppercase()));
                    }
                } else if k == "properties" {
                    if let Some(props) = v.as_object() {
                        let gemini_props: serde_json::Map<_, _> = props.iter()
                            .map(|(pk, pv)| (pk.clone(), to_gemini_schema(pv)))
                            .collect();
                        out.insert("properties".to_string(), serde_json::Value::Object(gemini_props));
                    }
                } else if k == "items" {
                    out.insert("items".to_string(), to_gemini_schema(v));
                } else {
                    out.insert(k.clone(), v.clone());
                }
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}

// ── OpenAI format ─────────────────────────────────────────────────────────────

fn export_openai(registry: &CapabilityRegistry, namespace: Option<&str>, all: bool) -> Result<()> {
    let caps = select_caps(registry, namespace, all);
    let tools: Vec<serde_json::Value> = caps.iter().map(|cap| openai_tool(cap)).collect();
    print_json(&tools);
    Ok(())
}

fn openai_tool(cap: &CapabilityManifest) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": sanitize_tool_name(&cap.name),
            "description": cap.description.as_deref().unwrap_or(&cap.name),
            "parameters": cap.input_schema,
        }
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn select_caps<'a>(registry: &'a CapabilityRegistry, namespace: Option<&str>, all: bool) -> Vec<&'a CapabilityManifest> {
    if let Some(ns) = namespace {
        registry.by_namespace(ns)
    } else if all {
        registry.all()
    } else {
        registry.all() // default: all (two-tool handles its own logic)
    }
}

/// Claude, Gemini, and OpenAI tool names must be `[a-zA-Z0-9_-]+` with max 64 chars.
/// Replace dots (namespace separator) with underscores.
fn sanitize_tool_name(name: &str) -> String {
    name.replace('.', "__").chars().take(64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_tool_name() {
        assert_eq!(sanitize_tool_name("git.status"), "git__status");
        assert_eq!(sanitize_tool_name("gcloud.aiplatform.models.list"), "gcloud__aiplatform__models__list");
    }

    #[test]
    fn test_gemini_schema_uppercase() {
        let schema = serde_json::json!({"type": "object", "properties": {"foo": {"type": "string"}}});
        let out = to_gemini_schema(&schema);
        assert_eq!(out["type"], "OBJECT");
        assert_eq!(out["properties"]["foo"]["type"], "STRING");
    }

    #[test]
    fn test_format_from_str() {
        assert!(matches!(ExportFormat::from_str("claude").unwrap(), ExportFormat::Claude));
        assert!(matches!(ExportFormat::from_str("gemini").unwrap(), ExportFormat::Gemini));
        assert!(matches!(ExportFormat::from_str("openai").unwrap(), ExportFormat::OpenAi));
        assert!(matches!(ExportFormat::from_str("two-tool").unwrap(), ExportFormat::TwoTool));
        assert!(ExportFormat::from_str("unknown").is_err());
    }
}
