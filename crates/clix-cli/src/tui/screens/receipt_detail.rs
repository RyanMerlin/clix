use ratatui::{prelude::*, widgets::*};
use clix_core::receipts::{Receipt, ReceiptStatus};
use crate::tui::theme;

pub fn render(f: &mut Frame, receipt: &Receipt, area: Rect) {
    let width = area.width.saturating_sub(4).max(60).min(100);
    let height = area.height.saturating_sub(4).max(20).min(40);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let dialog = Rect::new(x, y, width, height);

    f.render_widget(Clear, dialog);
    let id_short = {
        let s = receipt.id.to_string();
        s[..8.min(s.len())].to_string()
    };
    let title = format!(" Receipt {} ", id_short);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, theme::accent_bold()))
        .border_style(theme::border_focused());
    let inner = block.inner(dialog);
    f.render_widget(block, dialog);

    let mut lines: Vec<Line> = vec![Line::from("")];

    // Status badge
    let (status_icon, status_style) = match receipt.status {
        ReceiptStatus::Succeeded => ("✓ succeeded", theme::ok()),
        ReceiptStatus::Failed => ("✗ failed", theme::danger()),
        ReceiptStatus::Denied => ("⊘ denied", theme::warn()),
        ReceiptStatus::PendingApproval => ("… pending approval", theme::info()),
        ReceiptStatus::ApprovalDenied => ("✗ approval denied", theme::danger()),
    };
    lines.push(Line::from(vec![
        Span::styled("  status    ", theme::muted()),
        Span::styled(status_icon, status_style),
    ]));
    lines.push(kv("  capability", &receipt.capability));
    lines.push(kv("  decision  ", &receipt.decision));
    if let Some(ref reason) = receipt.reason {
        lines.push(kv("  reason    ", reason));
    }

    // Context
    let time = receipt.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string();
    lines.push(kv("  time      ", &time));
    if let Some(user) = receipt.context.get("user").and_then(|v| v.as_str()) {
        lines.push(kv("  user      ", user));
    }
    if let Some(profile) = receipt.context.get("profile").and_then(|v| v.as_str()) {
        lines.push(kv("  profile   ", profile));
    }

    // Execution
    lines.push(Line::from(""));
    if let Some(ref exec) = receipt.execution {
        if let Some(code) = exec.get("exitCode").and_then(|v| v.as_i64()) {
            lines.push(kv("  exit code ", &code.to_string()));
        }
        if let Some(tier) = exec.get("isolationTier").and_then(|v| v.as_str()) {
            lines.push(kv("  isolation ", tier));
        }
        let sandbox = if receipt.sandbox_enforced { "yes" } else { "no" };
        lines.push(kv("  sandbox   ", sandbox));
        if let Some(sha) = receipt.binary_sha256.as_deref() {
            lines.push(kv("  binary   ", &sha[..12.min(sha.len())]));
        }

        // stdout
        if let Some(out) = exec.get("stdout").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  stdout:", theme::muted())));
            for line in out.lines().take(8) {
                lines.push(Line::from(Span::styled(format!("    {}", line), theme::dim())));
            }
            if out.lines().count() > 8 {
                lines.push(Line::from(Span::styled("    …", theme::inactive())));
            }
        }
        // stderr
        if let Some(err) = exec.get("stderr").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  stderr:", theme::muted())));
            for line in err.lines().take(6) {
                lines.push(Line::from(Span::styled(format!("    {}", line), theme::danger())));
            }
            if err.lines().count() > 6 {
                lines.push(Line::from(Span::styled("    …", theme::inactive())));
            }
        }
    }

    // Footer
    let footer = if matches!(receipt.status, ReceiptStatus::PendingApproval) {
        "  esc: close   A: approve"
    } else {
        "  esc: close"
    };
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(footer, theme::muted())));

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn kv(key: &'static str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(key.to_string(), theme::muted()),
        Span::styled(value.to_string(), theme::normal()),
    ])
}
