//! TUI rendering for jhana-rs.
//!
//! Layout — pure vertical stacking for the Rock's 720×1280 portrait
//! display (≈45 cols × 40 rows at TerminusBold 32×16). Borrows from
//! the survey in `docs/14_TODO.md` task #20 (timr-tui's clock card +
//! header/footer chips, tui-big-text quadrant rendering, rdn's
//! double-border-for-focus, demo2's bg-colour status strip).
//!
//! ```text
//!  ── jhana  ────────────────── listening ──   ← status strip (bg = state colour)
//!                                                gap
//! ╔═══════════════════════════════════════════╗ ← focal card (double border)
//! ║                                           ║
//! ║   ████  ████  █████ ████  █  █  █████     ║   active-tool canvas:
//! ║   █  █  █     █     █  █  █  █    █       ║   - say(): big-text + mirror
//! ║   ████  ███   ████  ████  ████    █       ║   - listen(): listening + transcript
//! ║   █  █  █     █     █  █  █  █    █       ║   - pause(N): countdown big-text
//! ║   ████  ████  █████ ████  █  █    █       ║   - ring_bell(): ♪ glyph
//! ║                                           ║
//! ║   the body knows the breath already       ║   plain-text mirror of say()
//! ║                                           ║
//! ╚═══════════════════════════════════════════╝
//!                                                gap
//!   activity                                    ← log section label
//!   ─────────────────────────────────────────
//!   > you said: "loving-kindness please"        ← log, dim, newest at bottom
//!     spoke: "the body knows the breath..."
//!     tool: ring_bell()
//!     tool: pause(10s)
//!  ── back: quit  enter: speak  ↑↓: scroll ──   ← footer hint chip (dim)
//! ```

use std::collections::VecDeque;
use std::time::Instant;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use tui_big_text::{BigText, PixelSize};

// ---------- Theme ----------

/// Light, calm palette borrowed from gruvbox light-hard. One body
/// colour, one dim colour, one chip colour per state. Resist adding
/// more accents — the meditation surface stays calmer with restraint.
pub struct Theme {
    pub bg: Color,           // cream
    pub fg: Color,           // near-black, body text
    pub dim: Color,          // grey, log entries
    pub border: Color,       // border colour
    pub idle_bg: Color,      // warm grey strip
    pub listening_bg: Color, // soft green strip
    pub speaking_bg: Color,  // soft blue strip
    pub pausing_bg: Color,   // soft amber strip
    pub thinking_bg: Color,  // pale lavender strip
}

impl Theme {
    /// Gruvbox light-hard, single-accent calm palette.
    pub const fn calm_light() -> Self {
        Self {
            bg: Color::Rgb(0xf9, 0xf5, 0xd7),           // gruvbox bg0_h, cream
            fg: Color::Rgb(0x3c, 0x38, 0x36),           // gruvbox fg1, near-black
            dim: Color::Rgb(0x7c, 0x6f, 0x64),          // gruvbox gray, log
            border: Color::Rgb(0x68, 0x5a, 0x4e),       // darker gray for borders
            idle_bg: Color::Rgb(0xeb, 0xdb, 0xb2),      // bg2, warm grey
            listening_bg: Color::Rgb(0xb8, 0xbb, 0x26), // gruvbox green-bright
            speaking_bg: Color::Rgb(0x83, 0xa5, 0x98),  // gruvbox blue
            pausing_bg: Color::Rgb(0xd6, 0x5d, 0x0e),   // gruvbox orange
            thinking_bg: Color::Rgb(0xb1, 0x6c, 0xeb),  // gruvbox purple
        }
    }
}

// ---------- App state ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Models still warming up. Show the loading screen until both
    /// `stt::STT_READY` and `llm::LLM_READY` are set. ENTER ignored.
    Loading,
    /// Session not active. Waiting for ENTER.
    Idle,
    /// Agent is thinking — model is generating tokens for the next turn.
    Generating,
    /// Pause tool is sleeping (silent gap during meditation).
    Paused,
    /// Session finished. ENTER starts a new one.
    Done,
}

impl AppState {
    fn label(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Idle => "idle",
            Self::Generating => "thinking",
            Self::Paused => "pausing",
            Self::Done => "done",
        }
    }
}

/// Which agent tool is currently in flight. Drives the focal card
/// rendering — see `render_focal_card`.
#[derive(Debug, Clone)]
pub enum ActiveTool {
    Speaking,
    Listening,
    Pausing { ends_at: Instant, total_secs: f32 },
    RingingBell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    UserSpeech,
    AgentSpoke,
    ToolCall,
    System,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub kind: LogKind,
    pub text: String,
}

const MAX_LOG: usize = 200;

pub struct App {
    pub state: AppState,
    /// Most recent say() text — shown in the focal card.
    pub current_say: Option<String>,
    /// Which tool is currently dispatching.
    pub active_tool: Option<ActiveTool>,
    /// Activity log (newest at end).
    log: VecDeque<LogEntry>,
    /// Scroll offset within the log (0 = bottom).
    pub log_scroll: u16,
    pub theme: Theme,
    /// Total tokens generated this session — left intact for future
    /// status-line wiring, even though we removed the noisy stats row.
    pub token_count: u32,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::Loading,
            current_say: None,
            active_tool: None,
            log: VecDeque::with_capacity(MAX_LOG),
            log_scroll: 0,
            theme: Theme::calm_light(),
            token_count: 0,
        }
    }

    pub fn finish_loading(&mut self) {
        if self.state == AppState::Loading {
            self.state = AppState::Idle;
            self.current_say = None;
            self.active_tool = None;
        }
    }

    pub fn start_generating(&mut self) {
        self.state = AppState::Generating;
        self.token_count = 0;
    }

    pub fn finish(&mut self) {
        self.state = AppState::Done;
        self.active_tool = None;
    }

    pub fn reset(&mut self) {
        self.state = AppState::Idle;
        self.current_say = None;
        self.active_tool = None;
        self.token_count = 0;
        // Keep log — user can scroll back to see previous session.
    }

    /// Record a tool call dispatching. Sets the active-tool canvas state.
    pub fn note_tool_start(&mut self, name: &str, args: &serde_json::Value) {
        let display = format!("→ {name}{}", short_args(args));
        self.push_log(LogKind::ToolCall, display);
        self.active_tool = match name {
            "say" => {
                if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
                    self.current_say = Some(text.to_string());
                    self.push_log(LogKind::AgentSpoke, text.to_string());
                }
                Some(ActiveTool::Speaking)
            }
            "listen" => Some(ActiveTool::Listening),
            "pause" => {
                let secs = args.get("seconds").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let ends_at = Instant::now() + std::time::Duration::from_secs_f32(secs);
                self.state = AppState::Paused;
                Some(ActiveTool::Pausing {
                    ends_at,
                    total_secs: secs,
                })
            }
            "ring_bell" => Some(ActiveTool::RingingBell),
            _ => None,
        };
    }

    /// Record a tool result. Clears the active-tool canvas for tools
    /// that are now done; for listen/transcript results, surface the
    /// user's speech to the log.
    pub fn note_tool_result(&mut self, name: &str, ok: bool, snippet: &str) {
        if name == "listen" && ok {
            // Tease out the transcript field from the snippet which
            // looks like {"transcript":"hello teach me"}
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(snippet)
                && let Some(t) = v.get("transcript").and_then(|v| v.as_str())
            {
                self.push_log(LogKind::UserSpeech, t.to_string());
            }
        }
        let marker = if ok { "✓" } else { "✗" };
        self.push_log(LogKind::System, format!("{marker} {name}"));
        self.active_tool = None;
        // Coming out of pause returns to Generating until the next turn fires.
        if self.state == AppState::Paused {
            self.state = AppState::Generating;
        }
    }

    pub fn push_system(&mut self, text: String) {
        self.push_log(LogKind::System, text);
    }

    fn push_log(&mut self, kind: LogKind, text: String) {
        self.log.push_back(LogEntry { kind, text });
        while self.log.len() > MAX_LOG {
            self.log.pop_front();
        }
    }

    pub fn log_entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.log.iter()
    }

    pub fn scroll_up(&mut self) {
        self.log_scroll = self.log_scroll.saturating_add(1);
    }

    pub fn scroll_down(&mut self) {
        self.log_scroll = self.log_scroll.saturating_sub(1);
    }
}

/// Truncate JSON args for a one-line console display.
fn short_args(args: &serde_json::Value) -> String {
    if args.is_null()
        || (args.is_object() && args.as_object().is_some_and(serde_json::Map::is_empty))
    {
        "()".to_string()
    } else if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
        if text.len() > 40 {
            format!("(\"{}…\")", &text[..40])
        } else {
            format!("(\"{text}\")")
        }
    } else if let Some(secs) = args.get("seconds") {
        format!("({secs}s)")
    } else if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
        format!("(\"{name}\")")
    } else {
        let s = args.to_string();
        if s.len() > 30 {
            format!("({}…)", &s[..30])
        } else {
            format!("({s})")
        }
    }
}

// ---------- Rendering ----------

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = frame.area();

    // Background fill
    frame.render_widget(Block::default().style(Style::default().bg(t.bg)), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // status strip
            Constraint::Length(1),  // gap
            Constraint::Length(16), // focal card (border 2 + content 14)
            Constraint::Length(1),  // gap
            Constraint::Min(3),     // activity log
            Constraint::Length(1),  // footer
        ])
        .split(area);

    render_status_strip(frame, chunks[0], app);
    if app.state == AppState::Loading {
        render_loading_card(frame, chunks[2], app);
    } else {
        render_focal_card(frame, chunks[2], app);
    }
    render_log(frame, chunks[4], app);
    render_footer(frame, chunks[5], app);
}

fn render_status_strip(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let t = &app.theme;
    let bg = match app.state {
        AppState::Idle | AppState::Done => t.idle_bg,
        AppState::Generating => t.thinking_bg,
        AppState::Paused => t.pausing_bg,
        AppState::Loading => t.idle_bg,
    };
    let bg_override = match &app.active_tool {
        Some(ActiveTool::Speaking) => Some(t.speaking_bg),
        Some(ActiveTool::Listening) => Some(t.listening_bg),
        _ => None,
    };
    let final_bg = bg_override.unwrap_or(bg);

    // "── jhana ───────────────── <state> ──"
    let label = match (&app.active_tool, app.state) {
        (Some(ActiveTool::Speaking), _) => "speaking",
        (Some(ActiveTool::Listening), _) => "listening",
        (Some(ActiveTool::Pausing { .. }), _) => "pausing",
        (Some(ActiveTool::RingingBell), _) => "ringing",
        (None, s) => s.label(),
    };

    let title = format!(" ── jhana ");
    let suffix = format!(" {label} ── ");
    let middle_len = area
        .width
        .saturating_sub(title.chars().count() as u16)
        .saturating_sub(suffix.chars().count() as u16) as usize;
    let middle: String = "─".repeat(middle_len);

    let line = Line::from(vec![
        Span::styled(
            title,
            Style::default()
                .fg(t.fg)
                .bg(final_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(middle, Style::default().fg(t.fg).bg(final_bg)),
        Span::styled(
            suffix,
            Style::default()
                .fg(t.fg)
                .bg(final_bg)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(final_bg)),
        area,
    );
}

fn render_focal_card(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let t = &app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(t.border).bg(t.bg))
        .style(Style::default().bg(t.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner into big-text region + plain mirror region.
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Length(6), // big text (5 rows for Quadrant + 1 buffer)
            Constraint::Length(1), // gap
            Constraint::Min(2),    // plain-text mirror (wrapped)
            Constraint::Length(1), // bottom padding
        ])
        .split(inner);

    // Big-text content depends on active tool.
    let (big_lines, mirror_lines): (Vec<Line>, Vec<Line>) = match &app.active_tool {
        Some(ActiveTool::Pausing {
            ends_at,
            total_secs,
        }) => {
            let remaining = ends_at
                .saturating_duration_since(Instant::now())
                .as_secs_f32();
            let big = format!("{}", remaining.ceil() as u32);
            let mirror = format!(
                "silence  ·  {}/{}s",
                (total_secs - remaining).ceil() as u32,
                *total_secs as u32
            );
            (vec![Line::from(big)], vec![center_line(&mirror, &t.dim)])
        }
        Some(ActiveTool::RingingBell) => (vec![Line::from("♪")], vec![center_line("bell", &t.dim)]),
        Some(ActiveTool::Listening) => (
            vec![Line::from("listening")],
            vec![center_line("· · · ·", &t.dim)],
        ),
        _ => {
            // Default: show the most recent say() text. If none, show
            // a soft tagline.
            let text = app.current_say.as_deref().unwrap_or("be still").to_string();
            let big_short = pick_big_phrase(&text);
            let mirror = wrap_text(&text, inner_chunks[3].width as usize);
            let mirror_lines: Vec<Line> =
                mirror.into_iter().map(|s| center_line(&s, &t.fg)).collect();
            (vec![Line::from(big_short)], mirror_lines)
        }
    };

    let big_text = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::default().fg(t.fg).bg(t.bg))
        .alignment(Alignment::Center)
        .lines(big_lines)
        .build();
    frame.render_widget(big_text, inner_chunks[1]);

    let mirror = Paragraph::new(mirror_lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(t.bg));
    frame.render_widget(mirror, inner_chunks[3]);
}

fn render_loading_card(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let t = &app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(t.border).bg(t.bg))
        .style(Style::default().bg(t.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let big_text = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::default().fg(t.fg).bg(t.bg))
        .alignment(Alignment::Center)
        .lines(vec![Line::from("loading")])
        .build();
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(6),
            Constraint::Min(0),
        ])
        .split(inner);
    frame.render_widget(big_text, inner_chunks[1]);
    let stt_ready = crate::stt::STT_READY.load(std::sync::atomic::Ordering::Acquire);
    let stage = if stt_ready {
        "warming the meditation model (≈80 s)"
    } else {
        "warming the speech recogniser"
    };
    let p = Paragraph::new(stage)
        .alignment(Alignment::Center)
        .style(Style::default().fg(t.dim).bg(t.bg));
    frame.render_widget(p, inner_chunks[2]);
}

fn render_log(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let t = &app.theme;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // label
            Constraint::Length(1), // rule
            Constraint::Min(1),    // entries
        ])
        .split(area);

    let label = Paragraph::new(Line::from(Span::styled(
        " activity",
        Style::default().fg(t.dim).bg(t.bg),
    )))
    .style(Style::default().bg(t.bg));
    frame.render_widget(label, chunks[0]);

    let rule_text: String = std::iter::repeat('─').take(area.width as usize).collect();
    let rule = Paragraph::new(Line::from(Span::styled(
        rule_text,
        Style::default().fg(t.dim).bg(t.bg),
    )))
    .style(Style::default().bg(t.bg));
    frame.render_widget(rule, chunks[1]);

    // Render entries with newest at bottom, respecting log_scroll.
    let h = chunks[2].height as usize;
    let total: Vec<&LogEntry> = app.log_entries().collect();
    let skip_from_end = app.log_scroll as usize;
    let end = total.len().saturating_sub(skip_from_end);
    let start = end.saturating_sub(h);
    let lines: Vec<Line> = total[start..end]
        .iter()
        .map(|e| {
            let (prefix, style) = match e.kind {
                LogKind::UserSpeech => (
                    "  > you: ",
                    Style::default()
                        .fg(t.fg)
                        .bg(t.bg)
                        .add_modifier(Modifier::BOLD),
                ),
                LogKind::AgentSpoke => ("  spoke: ", Style::default().fg(t.fg).bg(t.bg)),
                LogKind::ToolCall => ("  ", Style::default().fg(t.dim).bg(t.bg)),
                LogKind::System => (
                    "  ",
                    Style::default()
                        .fg(t.dim)
                        .bg(t.bg)
                        .add_modifier(Modifier::DIM),
                ),
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(
                    truncate(&e.text, area.width.saturating_sub(12) as usize),
                    style,
                ),
            ])
        })
        .collect();
    let p = Paragraph::new(lines).style(Style::default().bg(t.bg));
    frame.render_widget(p, chunks[2]);
}

fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let t = &app.theme;
    let line = Line::from(vec![
        Span::styled(" ── ", Style::default().fg(t.dim).bg(t.bg)),
        Span::styled("back ", Style::default().fg(t.fg).bg(t.bg)),
        Span::styled("quit  ", Style::default().fg(t.dim).bg(t.bg)),
        Span::styled("enter ", Style::default().fg(t.fg).bg(t.bg)),
        Span::styled("start  ", Style::default().fg(t.dim).bg(t.bg)),
        Span::styled("↑↓ ", Style::default().fg(t.fg).bg(t.bg)),
        Span::styled("scroll ", Style::default().fg(t.dim).bg(t.bg)),
        Span::styled("──", Style::default().fg(t.dim).bg(t.bg)),
    ]);
    frame.render_widget(Paragraph::new(line).style(Style::default().bg(t.bg)), area);
}

// ---------- Helpers ----------

fn center_line<'a>(s: &str, fg: &Color) -> Line<'a> {
    Line::from(Span::styled(s.to_string(), Style::default().fg(*fg)))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Pick a short, ~10-char-or-less phrase from the say text to render
/// in the big-text region. Tries to find the first natural phrase
/// boundary (comma, period, dash), else truncates.
fn pick_big_phrase(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "be still".to_string();
    }
    // Find first phrase boundary
    for delim in [". ", ", ", " — ", "; ", "? ", "! "] {
        if let Some(idx) = trimmed.find(delim) {
            let chunk = &trimmed[..idx];
            if chunk.chars().count() <= 14 {
                return chunk.to_string();
            }
        }
    }
    // Fallback: take first 12 chars, break at word boundary if possible
    let chars: String = trimmed.chars().take(14).collect();
    if let Some(last_space) = chars.rfind(' ') {
        chars[..last_space].to_string()
    } else {
        chars
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            out.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
