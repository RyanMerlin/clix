//! Output formatting — JSON, table, YAML, CSV.
//!
//! Ported from the gws (Google Workspace CLI) formatter with minor adaptations
//! for clix's response shape (no `nextPageToken`, but same JSON value patterns).

use serde_json::Value;
use std::fmt::Write;

/// Supported output formats.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum OutputFormat {
    /// Pretty-printed JSON (default).
    #[default]
    Json,
    /// Aligned text table.
    Table,
    /// YAML.
    Yaml,
    /// Comma-separated values.
    Csv,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "table" => Ok(Self::Table),
            "yaml" | "yml" => Ok(Self::Yaml),
            "csv" => Ok(Self::Csv),
            other => Err(other.to_string()),
        }
    }

    pub fn from_str(s: &str) -> Self {
        Self::parse(s).unwrap_or(Self::Json)
    }
}

/// Format a JSON value according to the specified output format.
pub fn format_value(value: &Value, format: &OutputFormat) -> String {
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(value).unwrap_or_default(),
        OutputFormat::Table => format_table(value),
        OutputFormat::Yaml => format_yaml(value),
        OutputFormat::Csv => format_csv(value),
    }
}

pub fn print_json(value: &impl serde::Serialize) {
    println!("{}", serde_json::to_string_pretty(value).unwrap_or_else(|e| e.to_string()));
}

pub fn print_kv(rows: &[(&str, String)]) {
    let max_key = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (k, v) in rows {
        println!("{:<width$}  {v}", k, width = max_key);
    }
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

fn format_table(value: &Value) -> String {
    // Try to extract a list array from a wrapped object response
    let items = extract_items(value);

    if let Some((_key, arr)) = items {
        format_array_as_table(arr)
    } else if let Value::Array(arr) = value {
        format_array_as_table(arr)
    } else if let Value::Object(obj) = value {
        // Single object: key/value aligned table with flattened nested keys
        let mut output = String::new();
        let flat = flatten_object(obj, "");
        let max_key_len = flat.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (key, val_str) in &flat {
            let _ = writeln!(output, "{:width$}  {}", key, val_str, width = max_key_len);
        }
        output
    } else {
        value.to_string()
    }
}

fn format_array_as_table(arr: &[Value]) -> String {
    if arr.is_empty() {
        return "(empty)\n".to_string();
    }

    let flat_rows: Vec<Vec<(String, String)>> = arr
        .iter()
        .map(|item| match item {
            Value::Object(obj) => flatten_object(obj, ""),
            _ => vec![(String::new(), value_to_cell(item))],
        })
        .collect();

    // Collect unique column names in insertion order.
    let mut columns: Vec<String> = Vec::new();
    for row in &flat_rows {
        for (key, _) in row {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
    }

    if columns.is_empty() {
        let mut output = String::new();
        for item in arr {
            let _ = writeln!(output, "{}", value_to_cell(item));
        }
        return output;
    }

    let row_maps: Vec<std::collections::HashMap<&str, &str>> = flat_rows
        .iter()
        .map(|pairs| pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect())
        .collect();

    // Column widths, capped at 60 chars.
    let mut widths: Vec<usize> = columns.iter().map(|c| c.chars().count()).collect();
    let rows: Vec<Vec<String>> = row_maps
        .iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let cell = row.get(col.as_str()).copied().unwrap_or("").to_string();
                    let char_len = cell.chars().count();
                    if char_len > widths[i] {
                        widths[i] = char_len;
                    }
                    if widths[i] > 60 {
                        widths[i] = 60;
                    }
                    cell
                })
                .collect()
        })
        .collect();

    let mut output = String::new();

    // Header
    let header: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:width$}", c, width = widths[i]))
        .collect();
    let _ = writeln!(output, "{}", header.join("  "));

    // Separator
    let sep: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
    let _ = writeln!(output, "{}", sep.join("  "));

    // Rows — truncate by char count to stay within bounds safely.
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let char_len = c.chars().count();
                let truncated = if char_len > widths[i] {
                    let s: String = c.chars().take(widths[i] - 1).collect();
                    format!("{s}…")
                } else {
                    c.clone()
                };
                let pad = widths[i].saturating_sub(truncated.chars().count());
                format!("{truncated}{}", " ".repeat(pad))
            })
            .collect();
        let _ = writeln!(output, "{}", cells.join("  "));
    }

    output
}

fn flatten_object(obj: &serde_json::Map<String, Value>, prefix: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (key, val) in obj {
        let full_key = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        match val {
            Value::Object(nested) => out.extend(flatten_object(nested, &full_key)),
            _ => out.push((full_key, value_to_cell(val))),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// YAML
// ---------------------------------------------------------------------------

fn format_yaml(value: &Value) -> String {
    json_to_yaml(value, 0)
}

fn json_to_yaml(value: &Value, indent: usize) -> String {
    let prefix = "  ".repeat(indent);
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if s.contains('\n') {
                format!(
                    "|\n{}",
                    s.lines()
                        .map(|l| format!("{prefix}  {l}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            } else {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\"")
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() { return "[]".to_string(); }
            let mut out = String::new();
            for item in arr {
                let val_str = json_to_yaml(item, indent + 1);
                let _ = write!(out, "\n{prefix}- {val_str}");
            }
            out
        }
        Value::Object(obj) => {
            if obj.is_empty() { return "{}".to_string(); }
            let mut out = String::new();
            for (key, val) in obj {
                match val {
                    Value::Object(_) | Value::Array(_) => {
                        let val_str = json_to_yaml(val, indent + 1);
                        let _ = write!(out, "\n{prefix}{key}:{val_str}");
                    }
                    _ => {
                        let val_str = json_to_yaml(val, indent);
                        let _ = write!(out, "\n{prefix}{key}: {val_str}");
                    }
                }
            }
            out
        }
    }
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

fn format_csv(value: &Value) -> String {
    let items = extract_items(value);
    let arr = if let Some((_key, arr)) = items {
        arr.as_slice()
    } else if let Value::Array(arr) = value {
        arr.as_slice()
    } else {
        return value_to_cell(value);
    };

    if arr.is_empty() { return String::new(); }

    // Array of non-objects
    if !arr.iter().any(|v| v.is_object()) {
        let mut output = String::new();
        for item in arr {
            if let Value::Array(inner) = item {
                let cells: Vec<String> = inner.iter().map(|v| csv_escape(&value_to_cell(v))).collect();
                let _ = writeln!(output, "{}", cells.join(","));
            } else {
                let _ = writeln!(output, "{}", csv_escape(&value_to_cell(item)));
            }
        }
        return output;
    }

    let mut columns: Vec<String> = Vec::new();
    for item in arr {
        if let Value::Object(obj) = item {
            for key in obj.keys() {
                if !columns.contains(key) { columns.push(key.clone()); }
            }
        }
    }

    let mut output = String::new();
    let _ = writeln!(output, "{}", columns.join(","));
    for item in arr {
        let cells: Vec<String> = columns
            .iter()
            .map(|col| {
                if let Value::Object(obj) = item {
                    csv_escape(&value_to_cell(obj.get(col).unwrap_or(&Value::Null)))
                } else {
                    String::new()
                }
            })
            .collect();
        let _ = writeln!(output, "{}", cells.join(","));
    }
    output
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn extract_items(value: &Value) -> Option<(&str, &Vec<Value>)> {
    if let Value::Object(obj) = value {
        for (key, val) in obj {
            if key == "nextPageToken" || key == "kind" || key.starts_with('_') { continue; }
            if let Value::Array(arr) = val {
                if !arr.is_empty() { return Some((key, arr)); }
            }
        }
    }
    None
}

fn value_to_cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(arr) => arr.iter().map(value_to_cell).collect::<Vec<_>>().join(", "),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_output_format_parse() {
        assert_eq!(OutputFormat::parse("json"), Ok(OutputFormat::Json));
        assert_eq!(OutputFormat::parse("table"), Ok(OutputFormat::Table));
        assert_eq!(OutputFormat::parse("yaml"), Ok(OutputFormat::Yaml));
        assert_eq!(OutputFormat::parse("yml"), Ok(OutputFormat::Yaml));
        assert_eq!(OutputFormat::parse("csv"), Ok(OutputFormat::Csv));
        assert_eq!(OutputFormat::parse("JSON"), Ok(OutputFormat::Json));
        assert!(OutputFormat::parse("bogus").is_err());
    }

    #[test]
    fn test_format_table_array() {
        let val = json!({"items": [{"id": "1", "name": "foo"}, {"id": "2", "name": "bar"}]});
        let out = format_value(&val, &OutputFormat::Table);
        assert!(out.contains("id") && out.contains("name"));
        assert!(out.contains("foo") && out.contains("bar"));
        assert!(out.contains("──"));
    }

    #[test]
    fn test_format_table_single_object() {
        let val = json!({"id": "abc", "status": "ok"});
        let out = format_value(&val, &OutputFormat::Table);
        assert!(out.contains("id") && out.contains("abc"));
    }

    #[test]
    fn test_format_table_nested_flattened() {
        let val = json!([{"id": "1", "owner": {"name": "Alice"}}]);
        let out = format_value(&val, &OutputFormat::Table);
        assert!(out.contains("owner.name") && out.contains("Alice"));
    }

    #[test]
    fn test_format_csv() {
        let val = json!([{"id": "1", "name": "a"}, {"id": "2", "name": "b"}]);
        let out = format_value(&val, &OutputFormat::Csv);
        assert!(out.contains("id,name") && out.contains("1,a") && out.contains("2,b"));
    }

    #[test]
    fn test_format_yaml() {
        let val = json!({"name": "test", "count": 42});
        let out = format_value(&val, &OutputFormat::Yaml);
        assert!(out.contains("name: \"test\"") && out.contains("count: 42"));
    }

    #[test]
    fn test_format_table_multibyte_no_panic() {
        let val = json!([{"col": "😀".repeat(70)}]);
        format_value(&val, &OutputFormat::Table); // must not panic
    }
}
