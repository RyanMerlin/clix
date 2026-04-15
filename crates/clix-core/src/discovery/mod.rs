pub mod path_scan;
pub mod help_parse;
pub mod classify;

pub use path_scan::{scan_path, DiscoveredBinary};
pub use help_parse::{parse_help, ParsedSubcommand};
pub use classify::{classify, Classification};
