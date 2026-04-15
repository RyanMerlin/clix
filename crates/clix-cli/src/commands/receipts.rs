use anyhow::Result;
use clix_core::receipts::ReceiptStore;
use clix_core::state::{home_dir, ClixState};
use crate::output::print_json;
use std::io::Write;

pub fn list(limit: usize, status: Option<&str>, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    let receipts = store.list(limit, status)?;
    if json { print_json(&receipts); }
    else {
        for r in &receipts {
            println!("{} {} {} {}", r.id, r.created_at.format("%Y-%m-%dT%H:%M:%SZ"), r.status, r.capability);
        }
    }
    Ok(())
}

pub fn show(id: &str, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    match store.get(id)? {
        Some(r) if json => print_json(&r),
        Some(r) => {
            println!("id:          {}", r.id);
            println!("capability:  {}", r.capability);
            println!("status:      {}", r.status);
            println!("created:     {}", r.created_at);
            println!("sandbox:     {}", r.sandbox_enforced);
        }
        None => anyhow::bail!("receipt not found: {id}"),
    }
    Ok(())
}

pub fn export(status: Option<String>, since: Option<String>, format: String, output: Option<String>) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    let since_dt = if let Some(ref s) = since {
        let dt = chrono::DateTime::parse_from_rfc3339(s)
            .map_err(|e| anyhow::anyhow!("invalid --since timestamp: {e}"))?;
        Some(dt.with_timezone(&chrono::Utc))
    } else {
        None
    };
    let receipts = store.export(status.as_deref(), since_dt)?;
    let count = receipts.len();

    let mut writer: Box<dyn Write> = if let Some(ref path) = output {
        Box::new(std::fs::File::create(path)?)
    } else {
        Box::new(std::io::stdout())
    };

    if format == "json" {
        let arr = serde_json::to_string_pretty(&receipts)?;
        writeln!(writer, "{arr}")?;
    } else {
        // jsonl (default)
        for r in &receipts {
            let line = serde_json::to_string(r)?;
            writeln!(writer, "{line}")?;
        }
    }

    eprintln!("exported {count} receipts");
    Ok(())
}

pub fn tail() -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    let mut last_seen = String::new();
    loop {
        let receipts = store.list(50, None)?;
        for r in receipts.iter().rev() {
            let id = r.id.to_string();
            if id > last_seen {
                println!("{}", serde_json::to_string(r).unwrap_or_default());
                last_seen = id.clone();
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
