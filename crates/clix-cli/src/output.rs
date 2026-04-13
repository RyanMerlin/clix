pub fn print_json(value: &impl serde::Serialize) {
    println!("{}", serde_json::to_string_pretty(value).unwrap_or_else(|e| e.to_string()));
}

pub fn print_kv(rows: &[(&str, String)]) {
    let max_key = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (k, v) in rows {
        println!("{:<width$}  {v}", k, width = max_key);
    }
}
