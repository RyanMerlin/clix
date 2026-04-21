use ratatui::{prelude::*, widgets::*};
use clix_core::secrets::preview;
use crate::tui::app::App;
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([
            Constraint::Length(9),  // config card
            Constraint::Length(7),  // connectivity card
            Constraint::Min(5),     // bindings card
        ])
        .split(area);

    render_config_card(f, app, chunks[0]);
    render_connectivity_card(f, app, chunks[1]);
    render_bindings_card(f, app, chunks[2]);
}

fn render_config_card(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Infisical Configuration ", theme::accent_bold()))
        .border_style(theme::border_normal());

    let cfg = app.infisical_cfg.as_ref();
    let site_url = cfg.map(|c| c.site_url.as_str()).unwrap_or("(not set)");
    let project_id = cfg.and_then(|c| c.default_project_id.as_deref()).unwrap_or("");
    let environment = cfg.map(|c| c.default_environment.as_str()).unwrap_or("dev");

    // Determine auth method and credential source
    #[cfg(target_os = "linux")]
    let (auth_label, auth_value, auth_source) = {
        use clix_core::secrets::keyring;
        if keyring::load_service_token(&app.infisical_profile_name).is_some() {
            ("service_token", "(set)", "keyring")
        } else if cfg.and_then(|c| c.service_token.as_deref()).is_some() {
            ("service_token", "(set)", "config.yaml")
        } else if keyring::load_credentials(&app.infisical_profile_name).is_some() {
            ("machine_identity", "(set)", "keyring")
        } else if cfg.and_then(|c| c.client_id.as_deref()).is_some() {
            ("machine_identity", "(set)", "config.yaml")
        } else {
            ("auth", "(not configured)", "—")
        }
    };
    #[cfg(not(target_os = "linux"))]
    let (auth_label, auth_value, auth_source) = {
        if cfg.and_then(|c| c.service_token.as_deref()).is_some() {
            ("service_token", "(set)", "config.yaml")
        } else if cfg.and_then(|c| c.client_id.as_deref()).is_some() {
            ("machine_identity", "(set)", "config.yaml")
        } else {
            ("auth", "(not configured)", "—")
        }
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(""),
        kv_line("site_url", site_url, None),
        kv_line("project_id", &preview(project_id), None),
        kv_line("environment", environment, None),
        kv_line(auth_label, auth_value, Some(auth_source)),
        Line::from(""),
        Line::from(Span::styled("  m:accounts  e:edit  t:test connectivity", theme::muted())),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn kv_line<'a>(key: &'static str, value: &str, source: Option<&str>) -> Line<'a> {
    let mut spans = vec![
        Span::styled(format!("  {:16}", key), theme::muted()),
        Span::styled(value.to_string(), theme::normal()),
    ];
    if let Some(src) = source {
        spans.push(Span::styled(format!("  ({})", src), theme::inactive()));
    }
    Line::from(spans)
}

fn render_connectivity_card(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Connectivity ", theme::accent_bold()))
        .border_style(theme::border_dim());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = if let Some(ref report) = app.connectivity_report {
        let (badge, badge_style) = if report.auth_ok {
            ("● connected", theme::ok())
        } else {
            ("● error", theme::danger())
        };
        vec![
            Line::from(""),
            Line::from(Span::styled(format!("  {}", badge), badge_style)),
            Line::from(vec![
                Span::styled("  latency         ", theme::muted()),
                Span::styled(format!("{}ms", report.latency_ms), theme::normal()),
            ]),
            Line::from(vec![
                Span::styled("  token TTL       ", theme::muted()),
                Span::styled(
                    report.token_expires_in.map(|t| format!("{}s", t)).unwrap_or_else(|| "—".to_string()),
                    theme::normal(),
                ),
            ]),
            Line::from(vec![
                Span::styled("  root folders    ", theme::muted()),
                Span::styled(report.root_folder_count.to_string(), theme::normal()),
            ]),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(Span::styled("  ● unverified", theme::inactive())),
            Line::from(""),
            Line::from(Span::styled("  press t to test connectivity", theme::muted())),
        ]
    };
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_bindings_card(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Profile Bindings ", theme::accent_bold()))
        .border_style(theme::border_dim());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![Line::from("")];

    if app.profiles.is_empty() {
        lines.push(Line::from(Span::styled("  No profiles configured.", theme::muted())));
    } else {
        for profile in &app.profiles {
            let key_count = profile.secret_bindings.len();
            let folder_count = profile.folder_bindings.len();
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<20}", profile.name), theme::normal()),
                Span::styled(format!("{} secrets", key_count), theme::dim()),
                Span::styled(format!("  ({} folders)", folder_count), theme::muted()),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}
