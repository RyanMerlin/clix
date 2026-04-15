use crate::manifest::capability::{RiskLevel, SideEffectClass};

#[derive(Debug, Clone)]
pub struct Classification {
    pub risk: RiskLevel,
    pub side_effect: SideEffectClass,
}

static READ_VERBS: &[&str] = &[
    "list", "get", "describe", "show", "ls", "cat", "read", "status",
    "info", "head", "view", "display", "print", "fetch", "check",
    "inspect", "query", "find", "search", "scan", "diff", "log",
    "history", "audit", "verify",
];

static MUTATE_VERBS: &[&str] = &[
    "create", "add", "put", "set", "update", "write", "tag", "label",
    "mark", "edit", "modify", "change", "patch", "move", "copy",
    "install", "enable", "disable", "start", "stop", "restart", "reload",
    "apply", "deploy", "publish", "push", "upload", "sync",
];

static DESTRUCTIVE_VERBS: &[&str] = &[
    "delete", "rm", "remove", "destroy", "drop", "purge", "wipe",
    "kill", "terminate", "cancel", "revoke", "expire", "clear",
    "flush", "reset", "nuke", "erase", "clean",
];

static DESTRUCTIVE_FLAG_HINTS: &[&str] = &["--force", "--recursive", "--no-dry-run", "-rf", "-f"];

static SAFE_FLAG_HINTS: &[&str] = &["--dry-run", "--output", "--format", "--read-only"];

/// Classify a subcommand based on its name and the binary's overall flags/description.
pub fn classify(subcommand_dotted: &str, description: &str) -> Classification {
    // Take the last segment of "cmd.sub.subsub" for verb matching
    let last_part = subcommand_dotted.rsplit('.').next().unwrap_or(subcommand_dotted);
    let lower_name = last_part.to_lowercase();
    let lower_desc = description.to_lowercase();

    // Check for destructive signals first (conservative — flag high risk early)
    let is_destructive = DESTRUCTIVE_VERBS.iter().any(|v| {
        lower_name == *v || lower_name.starts_with(v) || lower_name.ends_with(v)
    }) || DESTRUCTIVE_FLAG_HINTS.iter().any(|f| lower_desc.contains(f));

    if is_destructive {
        return Classification {
            risk: RiskLevel::High,
            side_effect: SideEffectClass::Destructive,
        };
    }

    let is_mutate = MUTATE_VERBS.iter().any(|v| {
        lower_name == *v || lower_name.starts_with(v)
    });

    if is_mutate {
        // Bump risk if description hints at force/no-dry-run
        let risk = if lower_desc.contains("force") || lower_desc.contains("overwrite") {
            RiskLevel::High
        } else {
            RiskLevel::Medium
        };
        return Classification {
            risk,
            side_effect: SideEffectClass::Mutating,
        };
    }

    let is_read = READ_VERBS.iter().any(|v| {
        lower_name == *v || lower_name.starts_with(v)
    }) || SAFE_FLAG_HINTS.iter().any(|f| lower_desc.contains(f));

    if is_read {
        return Classification {
            risk: RiskLevel::Low,
            side_effect: SideEffectClass::ReadOnly,
        };
    }

    // Default: conservative medium risk, no side-effect classification
    Classification {
        risk: RiskLevel::Medium,
        side_effect: SideEffectClass::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_read() {
        let c = classify("aws.s3.list", "List S3 buckets");
        assert!(matches!(c.side_effect, SideEffectClass::ReadOnly));
        assert!(matches!(c.risk, RiskLevel::Low));
    }

    #[test]
    fn test_classify_destructive() {
        let c = classify("aws.s3.delete", "Delete an S3 object");
        assert!(matches!(c.side_effect, SideEffectClass::Destructive));
        assert!(matches!(c.risk, RiskLevel::High));
    }

    #[test]
    fn test_classify_mutate() {
        let c = classify("kubectl.apply", "Apply resources");
        assert!(matches!(c.side_effect, SideEffectClass::Mutating));
    }
}
