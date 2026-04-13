//! Dynamic CLI command tree builder.
//!
//! Converts the capability registry into a nested clap Command tree at startup,
//! mirroring the pattern used by the gws (Google Workspace CLI) project:
//! walk spec tree → build clap builder Commands.
//!
//! Dot-namespaced capability names become nested subcommands:
//!   "gcloud.aiplatform.models.list"  →  `clix gcloud aiplatform models list`
//!   "system.date"                    →  `clix system date`
//!
//! Note: clap 4.5+ requires 'static str for command/arg names when using the
//! builder API. We use String::leak() to promote owned strings to 'static.
//! This is intentional and bounded — capabilities load once per process.

use std::collections::BTreeMap;
use clap::{Arg, ArgMatches, Command};
use clix_core::manifest::capability::CapabilityManifest;
use clix_core::registry::CapabilityRegistry;

/// Internal trie node for building the nested command tree.
struct TrieNode {
    children: BTreeMap<String, TrieNode>,
    capability: Option<CapabilityManifest>,
}

impl TrieNode {
    fn new() -> Self {
        Self { children: BTreeMap::new(), capability: None }
    }
}

/// Static command names that must not be shadowed by dynamic capabilities.
const STATIC_COMMANDS: &[&str] = &[
    "init", "status", "version", "run", "capabilities",
    "workflow", "profile", "receipts", "serve", "pack",
];

/// Insert a capability into the trie by its dot-split segments.
fn insert(root: &mut BTreeMap<String, TrieNode>, parts: &[&str], cap: CapabilityManifest) {
    if parts.is_empty() { return; }
    let node = root.entry(parts[0].to_string()).or_insert_with(TrieNode::new);
    if parts.len() == 1 {
        node.capability = Some(cap);
    } else {
        insert(&mut node.children, &parts[1..], cap);
    }
}

/// Build a clap `Command` for a trie node and its children.
/// `name` must be `&'static str` (leak with `String::leak()` at call sites).
fn build_command(name: &'static str, node: &TrieNode) -> Option<Command> {
    let is_leaf = node.capability.is_some() && node.children.is_empty();

    let mut cmd = if is_leaf {
        let cap = node.capability.as_ref().unwrap();
        let about: &'static str = cap.description.clone()
            .unwrap_or_default()
            .leak();
        let mut c = Command::new(name).about(about);

        if let Some(props) = cap.input_schema.get("properties").and_then(|p| p.as_object()) {
            let required_set: std::collections::HashSet<String> = cap.input_schema
                .get("required").and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();

            for (key, schema) in props {
                let help: &'static str = schema.get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string()
                    .leak();
                let key_s: &'static str = key.clone().leak();
                let upper_s: &'static str = key.to_uppercase().leak();
                let req = required_set.contains(key);
                let mut arg = Arg::new(key_s).long(key_s).value_name(upper_s).help(help);
                if req { arg = arg.required(true); }
                c = c.arg(arg);
            }
        }
        c
    } else {
        let about: &'static str = node.capability.as_ref()
            .and_then(|c| c.description.clone())
            .unwrap_or_else(|| format!("'{name}' capabilities"))
            .leak();
        Command::new(name)
            .about(about)
            .subcommand_required(true)
            .arg_required_else_help(true)
    };

    let mut has_children = false;
    for (child_name, child_node) in &node.children {
        let child_name_s: &'static str = child_name.clone().leak();
        if let Some(child_cmd) = build_command(child_name_s, child_node) {
            cmd = cmd.subcommand(child_cmd);
            has_children = true;
        }
    }

    if !is_leaf && !has_children && node.capability.is_none() {
        return None;
    }

    Some(cmd)
}

/// Augment the root clap `Command` with dynamic subcommands from the capability registry.
/// Skips top-level segments that collide with static command names.
pub fn augment_with_capabilities(registry: &CapabilityRegistry, mut root: Command) -> Command {
    let mut trie: BTreeMap<String, TrieNode> = BTreeMap::new();

    for cap in registry.all() {
        let parts: Vec<&str> = cap.name.split('.').collect();
        if parts.is_empty() { continue; }
        if STATIC_COMMANDS.contains(&parts[0]) { continue; }
        insert(&mut trie, &parts, (*cap).clone());
    }

    for (name, node) in &trie {
        let name_s: &'static str = name.clone().leak();
        if let Some(cmd) = build_command(name_s, node) {
            root = root.subcommand(cmd);
        }
    }

    root
}

/// Walk clap's subcommand match chain and reconstruct the dot-joined capability name.
/// Returns `(capability_name, leaf_ArgMatches)` or `None` if no dynamic subcommand matched.
pub fn resolve_capability_name<'a>(matches: &'a ArgMatches) -> Option<(String, &'a ArgMatches)> {
    let mut path: Vec<String> = Vec::new();
    let mut current = matches;

    loop {
        match current.subcommand() {
            Some((name, sub)) => {
                if path.is_empty() && STATIC_COMMANDS.contains(&name) {
                    return None;
                }
                path.push(name.to_string());
                current = sub;
            }
            None => break,
        }
    }

    if path.is_empty() { return None; }
    Some((path.join("."), current))
}

/// Extract capability inputs from the leaf `ArgMatches` using the capability's inputSchema.
pub fn extract_inputs(matches: &ArgMatches, cap: &CapabilityManifest) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    if let Some(props) = cap.input_schema.get("properties").and_then(|p| p.as_object()) {
        for key in props.keys() {
            if let Some(val) = matches.get_one::<String>(key.as_str()) {
                let json_val = serde_json::from_str(val).unwrap_or(serde_json::Value::String(val.clone()));
                map.insert(key.clone(), json_val);
            }
        }
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clix_core::manifest::capability::{Backend, RiskLevel, SideEffectClass};

    fn make_cap(name: &str, desc: &str, props: serde_json::Value) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(), version: 1,
            description: Some(desc.to_string()),
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low, side_effect_class: SideEffectClass::ReadOnly,
            sandbox_profile: None, approval_policy: None,
            input_schema: serde_json::json!({"type":"object","properties": props}),
            validators: vec![], credentials: vec![],
        }
    }

    #[test]
    fn test_augment_creates_nested_subcommands() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("gcloud.aiplatform.models.list", "List models",
                serde_json::json!({"project":{"type":"string","description":"GCP project"}})),
            make_cap("system.date", "Get date", serde_json::json!({})),
        ]);
        let root = Command::new("clix");
        let cmd = augment_with_capabilities(&reg, root);

        assert!(cmd.find_subcommand("gcloud").is_some(), "gcloud should be a dynamic subcommand");
        assert!(cmd.find_subcommand("system").is_some(), "system should be a dynamic subcommand");
    }

    #[test]
    fn test_static_commands_not_shadowed() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("run.something", "run something", serde_json::json!({})),
            make_cap("gcloud.list-projects", "List projects", serde_json::json!({})),
        ]);
        let root = Command::new("clix");
        let cmd = augment_with_capabilities(&reg, root);
        // "run" is static — dynamic entry must be suppressed
        assert!(cmd.find_subcommand("run").is_none());
        assert!(cmd.find_subcommand("gcloud").is_some());
    }

    #[test]
    fn test_resolve_capability_name() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("gcloud.aiplatform.models.list", "List models",
                serde_json::json!({"project":{"type":"string","description":"GCP project"}})),
        ]);
        let root = Command::new("clix");
        let cmd = augment_with_capabilities(&reg, root);

        let matches = cmd.try_get_matches_from(
            ["clix", "gcloud", "aiplatform", "models", "list", "--project", "my-project"]
        ).unwrap();
        let (name, leaf) = resolve_capability_name(&matches).unwrap();
        assert_eq!(name, "gcloud.aiplatform.models.list");
        assert_eq!(leaf.get_one::<String>("project").map(|s| s.as_str()), Some("my-project"));
    }

    #[test]
    fn test_static_command_not_resolved_as_dynamic() {
        let reg = CapabilityRegistry::from_vec(vec![]);
        let root = Command::new("clix").subcommand(Command::new("version"));
        let cmd = augment_with_capabilities(&reg, root);
        let matches = cmd.try_get_matches_from(["clix", "version"]).unwrap();
        assert!(resolve_capability_name(&matches).is_none());
    }
}
