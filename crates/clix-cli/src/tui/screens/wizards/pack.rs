use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use clix_core::discovery::{DiscoveredBinary, ParsedSubcommand, Classification};
use clix_core::packs::scaffold::Preset;
use crate::tui::theme;
use crate::tui::widgets::checklist::{Checklist, ChecklistItem};
use crate::tui::widgets::form::{FieldInput, SelectField};
use super::profile::render_text_field;

#[derive(Debug, Clone, PartialEq)]
pub enum PackWizardStep {
    Identity,       // step 0
    DiscoverBinaries, // step 1
    SelectSubcmds,  // step 2
    Preview,        // step 3
}

#[derive(Debug, Clone)]
pub struct DiscoveredSubcmd {
    pub parsed: ParsedSubcommand,
    pub classification: Classification,
}

#[derive(Debug, Clone)]
pub struct PackWizard {
    pub step: PackWizardStep,
    // Step 0 — identity
    pub name: FieldInput,
    pub description: FieldInput,
    pub author: FieldInput,
    pub seed_command: FieldInput,
    pub preset: SelectField,
    pub active_field: usize,  // 0=name,1=desc,2=author,3=seed_cmd,4=preset
    // Step 1 — discover binaries
    pub all_binaries: Vec<DiscoveredBinary>,
    pub binary_checklist: Checklist,
    pub scanning: bool,
    // Step 2 — subcommands
    pub all_subcmds: Vec<DiscoveredSubcmd>,
    pub subcmd_checklist: Checklist,
    pub parsing_help: bool,
    pub heuristic_filter: bool,
    // Error
    pub error: Option<String>,
}

pub enum PackWizardAction {
    None,
    Cancel,
    Submit {
        name: String,
        description: String,
        author: String,
        preset: Preset,
        seed_command: String,
        capability_names: Vec<String>,
    },
}

impl PackWizard {
    pub fn new() -> Self {
        let mut wiz = Self {
            step: PackWizardStep::Identity,
            name: FieldInput::default(),
            description: FieldInput::default(),
            author: FieldInput::default(),
            seed_command: FieldInput::default(),
            preset: SelectField::new(vec!["read-only", "change-controlled", "operator"]),
            active_field: 0,
            all_binaries: Vec::new(),
            binary_checklist: Checklist::new(vec![]),
            scanning: false,
            all_subcmds: Vec::new(),
            subcmd_checklist: Checklist::new(vec![]),
            parsing_help: false,
            heuristic_filter: true,
            error: None,
        };
        // Try to pre-fill author from git config
        if let Ok(out) = std::process::Command::new("git")
            .args(["config", "--get", "user.email"])
            .output()
        {
            let email = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !email.is_empty() {
                wiz.author = FieldInput::new(&email);
            }
        }
        wiz
    }

    pub fn is_dirty(&self) -> bool {
        !self.name.value.is_empty()
    }

    pub fn handle_key(&mut self, code: KeyCode) -> PackWizardAction {
        match &self.step {
            PackWizardStep::Identity => self.handle_identity(code),
            PackWizardStep::DiscoverBinaries => self.handle_discover(code),
            PackWizardStep::SelectSubcmds => self.handle_subcmds(code),
            PackWizardStep::Preview => self.handle_preview(code),
        }
    }

    fn handle_identity(&mut self, code: KeyCode) -> PackWizardAction {
        const FIELDS: usize = 5;
        match code {
            KeyCode::Esc => return PackWizardAction::Cancel,
            KeyCode::Tab => self.active_field = (self.active_field + 1) % FIELDS,
            KeyCode::BackTab => self.active_field = self.active_field.checked_sub(1).unwrap_or(FIELDS - 1),
            KeyCode::Enter => {
                let name = self.name.value.trim().to_string();
                if name.is_empty() {
                    self.error = Some("Name is required".into());
                    return PackWizardAction::None;
                }
                if name.contains('/') || name.contains('\\') || name.contains("..") {
                    self.error = Some("Name must not contain path separators".into());
                    return PackWizardAction::None;
                }
                self.error = None;
                // Sync binaries scan when moving to step 1
                self.start_scan();
                self.step = PackWizardStep::DiscoverBinaries;
            }
            _ => {
                self.error = None;
                match self.active_field {
                    0 => self.name.handle_key(code),
                    1 => self.description.handle_key(code),
                    2 => self.author.handle_key(code),
                    3 => self.seed_command.handle_key(code),
                    4 => self.preset.handle_key(code),
                    _ => {}
                }
            }
        }
        PackWizardAction::None
    }

    fn start_scan(&mut self) {
        self.scanning = true;
        self.all_binaries = clix_core::discovery::scan_path();
        let items: Vec<ChecklistItem> = self.all_binaries.iter().map(|b| {
            let size_str = human_size(b.size_bytes);
            ChecklistItem::new(
                &b.name,
                &b.name,
                &b.path.to_string_lossy(),
                &size_str,
                theme::TEXT_DIM,
                "",
            )
        }).collect();
        self.binary_checklist = Checklist::new(items);
        // Pre-filter and pre-select based on seed_command
        let seed = self.seed_command.value.trim().to_lowercase();
        if !seed.is_empty() {
            // Set filter so the list opens showing only matching binaries
            self.binary_checklist.filter = seed.clone();
            // Auto-select exact match
            for item in &mut self.binary_checklist.items {
                if item.label.to_lowercase() == seed {
                    item.selected = true;
                }
            }
        }
        self.scanning = false;
    }

    fn handle_discover(&mut self, code: KeyCode) -> PackWizardAction {
        match code {
            KeyCode::Esc => self.step = PackWizardStep::Identity,
            KeyCode::Enter => {
                // Parse help for selected binaries
                self.parse_selected_help();
                self.step = PackWizardStep::SelectSubcmds;
            }
            _ => { self.binary_checklist.handle_key(code); }
        }
        PackWizardAction::None
    }

    fn parse_selected_help(&mut self) {
        self.parsing_help = true;
        let selected_names: Vec<String> = self.binary_checklist.selected_ids();

        let mut subcmds: Vec<DiscoveredSubcmd> = Vec::new();
        for name in &selected_names {
            let parsed = clix_core::discovery::parse_help(name);
            for p in parsed {
                let cls = clix_core::discovery::classify(&p.name, &p.description);
                subcmds.push(DiscoveredSubcmd { parsed: p, classification: cls });
            }
        }

        // If no subcommands found, create one from the binary itself
        if subcmds.is_empty() {
            for name in &selected_names {
                let cls = clix_core::discovery::classify(&format!("{}.run", name), "");
                let pack_name = self.name.value.trim().to_string();
                subcmds.push(DiscoveredSubcmd {
                    parsed: ParsedSubcommand {
                        name: format!("{}.run", pack_name.replace('-', "_")),
                        description: format!("Run {}", name),
                    },
                    classification: cls,
                });
            }
        }

        // Apply heuristic filter based on preset
        let preset_str = self.preset.current().to_string();
        let items: Vec<ChecklistItem> = subcmds.iter().map(|sc| {
            let risk_str = match sc.classification.risk {
                clix_core::manifest::capability::RiskLevel::Low => "low",
                clix_core::manifest::capability::RiskLevel::Medium => "med",
                clix_core::manifest::capability::RiskLevel::High => "high",
                clix_core::manifest::capability::RiskLevel::Critical => "crit",
            };
            let se_str = match sc.classification.side_effect {
                clix_core::manifest::capability::SideEffectClass::None => "—",
                clix_core::manifest::capability::SideEffectClass::ReadOnly => "read",
                clix_core::manifest::capability::SideEffectClass::Additive => "add",
                clix_core::manifest::capability::SideEffectClass::Mutating => "mutate",
                clix_core::manifest::capability::SideEffectClass::Destructive => "destr",
            };
            let tag = format!("{:<4} {}", risk_str, se_str);
            let tag_color = theme::risk_color(risk_str);

            let mut item = ChecklistItem::new(
                &sc.parsed.name,
                &sc.parsed.name,
                &sc.parsed.description,
                &tag,
                tag_color,
                "",
            );
            // Auto-select based on preset heuristic
            item.selected = match preset_str.as_str() {
                "read-only" => matches!(sc.classification.side_effect,
                    clix_core::manifest::capability::SideEffectClass::ReadOnly |
                    clix_core::manifest::capability::SideEffectClass::None),
                "change-controlled" => !matches!(sc.classification.risk,
                    clix_core::manifest::capability::RiskLevel::Critical),
                _ => true,
            };
            item
        }).collect();

        self.all_subcmds = subcmds;
        self.subcmd_checklist = Checklist::new(items);
        self.parsing_help = false;
    }

    fn handle_subcmds(&mut self, code: KeyCode) -> PackWizardAction {
        match code {
            KeyCode::Esc => self.step = PackWizardStep::DiscoverBinaries,
            KeyCode::Enter => {
                self.step = PackWizardStep::Preview;
            }
            KeyCode::Char('h') => {
                self.heuristic_filter = !self.heuristic_filter;
            }
            _ => { self.subcmd_checklist.handle_key(code); }
        }
        PackWizardAction::None
    }

    fn handle_preview(&mut self, code: KeyCode) -> PackWizardAction {
        match code {
            KeyCode::Esc => self.step = PackWizardStep::SelectSubcmds,
            KeyCode::Enter | KeyCode::Char('w') => {
                let preset = match self.preset.current() {
                    "change-controlled" => Preset::ChangeControlled,
                    "operator" => Preset::Operator,
                    _ => Preset::ReadOnly,
                };
                return PackWizardAction::Submit {
                    name: self.name.value.trim().to_string(),
                    description: self.description.value.trim().to_string(),
                    author: self.author.value.trim().to_string(),
                    preset,
                    seed_command: self.seed_command.value.trim().to_string(),
                    capability_names: self.subcmd_checklist.selected_ids(),
                };
            }
            _ => {}
        }
        PackWizardAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(4).max(55);
        let height = area.height.saturating_sub(2).max(14);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);

        f.render_widget(Clear, dialog);

        let step_n = match self.step {
            PackWizardStep::Identity => 0,
            PackWizardStep::DiscoverBinaries => 1,
            PackWizardStep::SelectSubcmds => 2,
            PackWizardStep::Preview => 3,
        };
        let step_label = match self.step {
            PackWizardStep::Identity => "Identity",
            PackWizardStep::DiscoverBinaries => "Discover commands",
            PackWizardStep::SelectSubcmds => "Select capabilities",
            PackWizardStep::Preview => "Preview & write",
        };
        let dots = (0..4usize).map(|i| if i == step_n { "●" } else { "○" }).collect::<Vec<_>>().join("");
        let title = format!(" New Pack · {} {} ", dots, step_label);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        match &self.step {
            PackWizardStep::Identity => self.render_identity(f, inner),
            PackWizardStep::DiscoverBinaries => self.render_discover(f, inner),
            PackWizardStep::SelectSubcmds => self.render_subcmds(f, inner),
            PackWizardStep::Preview => self.render_preview(f, inner),
        }
    }

    fn render_identity(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),  // name
                Constraint::Length(3),  // description
                Constraint::Length(3),  // author
                Constraint::Length(3),  // seed command
                Constraint::Length(5),  // preset radio
                Constraint::Length(1),  // error/hint
                Constraint::Min(0),
            ])
            .split(area);

        render_text_field(f, &self.name, "Pack name *", self.active_field == 0, chunks[0]);
        render_text_field(f, &self.description, "Description", self.active_field == 1, chunks[1]);
        render_text_field(f, &self.author, "Author", self.active_field == 2, chunks[2]);
        render_text_field(f, &self.seed_command, "Seed command (optional, pre-selects in discovery)", self.active_field == 3, chunks[3]);

        // Preset radio group
        render_preset(f, &self.preset, self.active_field == 4, chunks[4]);

        if let Some(err) = &self.error {
            f.render_widget(Paragraph::new(Span::styled(err.as_str(), theme::danger())), chunks[5]);
        } else {
            f.render_widget(
                Paragraph::new("tab: next field   ← →: change preset   enter: continue   esc: cancel")
                    .style(theme::muted()),
                chunks[5]
            );
        }
    }

    fn render_discover(&self, f: &mut Frame, area: Rect) {
        if self.scanning {
            let msg = Paragraph::new(Span::styled("  Scanning $PATH …", theme::muted()));
            f.render_widget(msg, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let count_str = format!("{} executables found", self.all_binaries.len());
        let title = format!("Binaries — {}", count_str);
        self.binary_checklist.render(f, chunks[0], &title, true);

        let hint = Paragraph::new("↑↓ move   space toggle   / filter   a all   enter next   esc back")
            .style(theme::muted());
        f.render_widget(hint, chunks[1]);
    }

    fn render_subcmds(&self, f: &mut Frame, area: Rect) {
        if self.parsing_help {
            let msg = Paragraph::new(Span::styled("  Parsing --help output …", theme::muted()));
            f.render_widget(msg, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let title = format!("Capabilities — {} found", self.all_subcmds.len());
        self.subcmd_checklist.render(f, chunks[0], &title, true);

        let hint = Paragraph::new(
            "↑↓ move   space toggle   / filter   a all   h toggle-heuristic   enter next   esc back"
        ).style(theme::muted());
        f.render_widget(hint, chunks[1]);
    }

    fn render_preview(&self, f: &mut Frame, area: Rect) {
        let selected = self.subcmd_checklist.selected_ids();
        let name = self.name.value.trim();
        let preset_str = self.preset.current();

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Pack name    ", theme::muted()),
                Span::styled(name, theme::accent()),
            ]),
            Line::from(vec![
                Span::styled("  Preset       ", theme::muted()),
                Span::styled(preset_str, theme::normal()),
            ]),
            Line::from(vec![
                Span::styled("  Capabilities ", theme::muted()),
                Span::styled(format!("{} will be written", selected.len()), theme::normal()),
            ]),
            Line::from(""),
        ];

        let pack_name_rust = name.replace('-', "_");
        lines.push(Line::from(Span::styled(format!("  ~/.clix/packs/{}/", name), theme::dim())));
        lines.push(Line::from(Span::styled(format!("    pack.yaml"), theme::dim())));
        lines.push(Line::from(Span::styled(format!("    capabilities/"), theme::dim())));
        lines.push(Line::from(Span::styled(format!("      {}.version.yaml  (seed)", pack_name_rust), theme::muted())));
        for cap in selected.iter().take(4) {
            lines.push(Line::from(Span::styled(format!("      {}.yaml", cap), theme::muted())));
        }
        if selected.len() > 4 {
            lines.push(Line::from(Span::styled(format!("      … {} more", selected.len() - 4), theme::muted())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  enter", theme::accent()),
            Span::raw(" write pack   "),
            Span::styled("esc", theme::muted()),
            Span::raw(" back"),
        ]));

        let para = Paragraph::new(lines).wrap(Wrap { trim: false });
        f.render_widget(para, area);
    }
}

fn render_preset(f: &mut Frame, field: &SelectField, focused: bool, area: Rect) {
    let border_style = if focused { theme::border_focused() } else { theme::border_normal() };
    let descriptions = ["no writes, no approvals", "writes require approval", "full trust, operators only"];
    let options = field.options.iter().enumerate().map(|(i, opt)| {
        let bullet = if i == field.idx { "●" } else { "○" };
        let style = if i == field.idx { theme::accent() } else { theme::muted() };
        let desc = descriptions.get(i).copied().unwrap_or("");
        Line::from(vec![
            Span::styled(format!("  {} ", bullet), style),
            Span::styled(format!("{:<20}", opt), style),
            Span::styled(desc, theme::muted()),
        ])
    }).collect::<Vec<_>>();

    let para = Paragraph::new(options)
        .block(Block::default().borders(Borders::ALL).title("Preset").border_style(border_style));
    f.render_widget(para, area);
}

fn human_size(bytes: u64) -> String {
    if bytes == 0 { return "0 B".into(); }
    let units = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", size, units[unit])
}
