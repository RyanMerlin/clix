use anyhow::Result;
use clix_core::receipts::ReceiptStore;
use clix_core::state::{home_dir, ClixState};
use crate::output::print_json;

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
