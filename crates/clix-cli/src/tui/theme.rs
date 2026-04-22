use ratatui::style::{Color, Modifier, Style};

// Primary accent — rust orange
pub const ACCENT_BRIGHT: Color = Color::Rgb(231, 91, 42);

// Neutral chrome
pub const BORDER: Color = Color::Rgb(80, 80, 80);
pub const BORDER_DIM: Color = Color::Rgb(55, 55, 55);
pub const TEXT: Color = Color::White;
pub const TEXT_DIM: Color = Color::Rgb(140, 140, 140);
pub const TEXT_MUTED: Color = Color::Rgb(102, 102, 102);
pub const TEXT_INACTIVE: Color = Color::Rgb(70, 70, 70);

// Semantic
pub const OK: Color = Color::Rgb(92, 184, 92);
pub const WARN: Color = Color::Rgb(240, 173, 78);
pub const DANGER: Color = Color::Rgb(217, 83, 79);
pub const INFO: Color = Color::Rgb(106, 166, 255);

// Selection highlight (replaces flat DarkGray background)
pub const SELECTED_BG: Color = Color::Rgb(50, 30, 25);  // dark rust tint
pub const SELECTED_FG: Color = ACCENT_BRIGHT;

// Style helpers
pub fn accent() -> Style { Style::default().fg(ACCENT_BRIGHT) }
pub fn accent_bold() -> Style { Style::default().fg(ACCENT_BRIGHT).add_modifier(Modifier::BOLD) }
pub fn muted() -> Style { Style::default().fg(TEXT_MUTED) }
pub fn dim() -> Style { Style::default().fg(TEXT_DIM) }
pub fn inactive() -> Style { Style::default().fg(TEXT_INACTIVE) }
pub fn ok() -> Style { Style::default().fg(OK) }
pub fn warn() -> Style { Style::default().fg(WARN) }
pub fn danger() -> Style { Style::default().fg(DANGER) }
pub fn info() -> Style { Style::default().fg(INFO) }
pub fn normal() -> Style { Style::default().fg(TEXT) }

pub fn selected() -> Style {
    Style::default()
        .fg(SELECTED_FG)
        .bg(SELECTED_BG)
        .add_modifier(Modifier::BOLD)
}

pub fn border_focused() -> Style { Style::default().fg(ACCENT_BRIGHT) }
pub fn border_normal() -> Style { Style::default().fg(BORDER) }
pub fn border_dim() -> Style { Style::default().fg(BORDER_DIM) }
pub fn border_for(focused: bool) -> Style { if focused { border_focused() } else { border_normal() } }

/// Risk-level color
pub fn risk_color(risk: &str) -> Color {
    match risk {
        "low" | "Low" => OK,
        "medium" | "Medium" => WARN,
        "high" | "High" | "critical" | "Critical" => DANGER,
        _ => TEXT_DIM,
    }
}

/// Side-effect color
pub fn side_effect_color(class: &str) -> Color {
    match class {
        "readOnly" | "ReadOnly" | "read_only" => OK,
        "none" | "None" => TEXT_DIM,
        "additive" | "Additive" => INFO,
        "mutating" | "Mutating" => WARN,
        "destructive" | "Destructive" => DANGER,
        _ => TEXT_DIM,
    }
}
