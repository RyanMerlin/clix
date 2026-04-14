use crate::error::Result;
pub fn apply_sandbox(_allowed: &[String]) -> Result<()> { Ok(()) }
pub fn sandbox_enforced() -> bool { false }
