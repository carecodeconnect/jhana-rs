//! TUI rendering for jhana-rs using ratatui.
//!
//! Ratatui was chosen over a graphical toolkit because it runs in any
//! terminal with no X11, Wayland, or GPU driver — critical for the Rock 5A
//! which renders directly to a DRM/KMS framebuffer console.
//!
//! The retro phosphor-green/amber palette is deliberate: it's calming,
//! high-contrast on the small 720x1280 portrait display, and fun during
//! development. The layout targets 45 columns x 40 rows (`TerminusBold`
//! 32x16 console font — the largest available, chosen to approximate the
//! original pygame captioning service's 70px Noto font).
//!
//! # Layout
//!
//! ```text
//! ┌── jhana-rs ─────────────────────┐
//! │  ༄  jhana-rs  ～ breathe ～     │
//! ├─────────────────────────────────┤
//! │                                 │
//! │  Close your eyes and take a     │
//! │  deep breath in.                │
//! │                                 │
//! │  · · · 5s · · ·                 │
//! │                                 │
//! ├─────────────────────────────────┤
//! │  ◈ Generating  47 tok  6.2 t/s  │
//! │  ←quit →start ↑up ↓down q:quit │
//! └─────────────────────────────────┘
//! ```

use std::collections::VecDeque;
use std::time::Instant;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Color theme for the TUI.
///
/// RGB colors are used instead of terminal palette indices because the
/// Rock's `TERM=linux` console supports 24-bit color via DRM/KMS, and
/// fixed RGB values ensure consistent appearance regardless of terminal
/// theme.
pub struct Theme {
    /// Background color for all widgets.
    pub bg: Color,
    /// Primary accent — titles, highlights.
    pub accent: Color,
    /// Dim accent — secondary text, borders.
    pub accent_dim: Color,
    /// Title secondary — "breathe" subtitle.
    pub title_dim: Color,
    /// Body text — meditation sentences.
    pub body: Color,
    /// Pause marker styling.
    pub pause: Color,
    /// Status text — state label.
    pub status: Color,
}

impl Theme {
    /// Light theme — white background, retro green/amber accents.
    /// High contrast for outdoor use on the Rock's 720x1280 display.
    /// Inspired by classic terminal-on-white (x61s i3 style).
    pub const fn light() -> Self {
        Self {
            bg: Color::White,           // standard white bg (works on TERM=linux)
            accent: Color::DarkGray,    // dark gray — titles, button labels
            accent_dim: Color::Gray,    // gray — borders, secondary text
            title_dim: Color::DarkGray, // subtitle
            body: Color::Black,         // black — main meditation text
            pause: Color::DarkGray,     // pause markers
            status: Color::DarkGray,    // status label
        }
    }

    /// Dark theme — black background, phosphor green/amber.
    /// Classic retro CRT look. Better for indoor/dim environments.
    #[expect(dead_code)] // available for user toggle, not yet wired
    pub const fn dark() -> Self {
        Self {
            bg: Color::Reset,                    // terminal default (black)
            accent: Color::Rgb(255, 176, 0),     // amber — titles, highlights
            accent_dim: Color::Rgb(40, 110, 60), // dim green — borders
            title_dim: Color::Rgb(160, 110, 0),  // dim amber — subtitle
            body: Color::Rgb(200, 200, 180),     // soft white — main text
            pause: Color::Rgb(120, 100, 180),    // purple — pause markers
            status: Color::Rgb(80, 220, 120),    // phosphor green — status
        }
    }
}

/// Application lifecycle state.
///
/// Tracks where we are in the meditation flow so the TUI can show
/// appropriate status and the event loop can gate button actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Waiting for user to press START. No LLM activity.
    Idle,
    /// LLM is streaming tokens. Sentences appear one by one.
    Generating,
    /// A `[pause N]` marker is active. Countdown in progress.
    Paused,
    /// LLM finished generating. All text displayed.
    Done,
}

impl std::fmt::Display for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Generating => write!(f, "Generating"),
            Self::Paused => write!(f, "Paused"),
            Self::Done => write!(f, "Done"),
        }
    }
}

/// Application state displayed in the TUI.
///
/// Maximum number of text lines retained in memory. Older lines are
/// dropped from the front of the deque so a long-running session does
/// not grow unbounded — see docs/11_BENCHMARKS.md "RAM efficiency".
const MAX_LINES: usize = 200;

/// Holds the meditation text, scroll position, generation stats, and
/// lifecycle state. Scroll offset exists because the Rock's 720x1280
/// display at 32px font height gives only ~34 visible body rows —
/// longer meditations need scrolling.
pub struct App {
    /// All generated lines (the full meditation text so far).
    /// New lines are appended as the LLM streams them; once the buffer
    /// reaches `MAX_LINES` the oldest lines are dropped from the front
    /// to keep peak RSS bounded. `Box<str>` is used (rather than
    /// `String`) because completed lines never grow and `Box<str>`
    /// drops the unused capacity word.
    all_lines: VecDeque<Box<str>>,
    /// Number of lines currently visible. During generation, this
    /// increases one sentence at a time for a reveal effect.
    /// When idle/done, equals `all_lines.len()`.
    visible_count: usize,
    /// Current lifecycle state.
    pub state: AppState,
    /// Vertical scroll offset into the text (0 = top).
    pub scroll: u16,
    /// Total tokens generated in the current session.
    pub token_count: u32,
    /// When the current generation started (for tokens/sec calculation).
    generation_start: Option<Instant>,
    /// Active pause countdown: seconds remaining. `None` if not pausing.
    pub pause_remaining: Option<f32>,
    /// Color theme — light (outdoor) or dark (indoor/retro CRT).
    pub theme: Theme,
}

impl App {
    /// Create a new [`App`] in [`AppState::Idle`] with the light theme (outdoor).
    pub fn new() -> Self {
        Self {
            all_lines: VecDeque::with_capacity(MAX_LINES),
            visible_count: 0,
            state: AppState::Idle,
            scroll: 0,
            token_count: 0,
            generation_start: None,
            pause_remaining: None,
            theme: Theme::light(),
        }
    }

    /// Lines currently visible in the TUI (for sentence-by-sentence reveal).
    pub fn visible_lines(&self) -> impl Iterator<Item = &str> {
        let end = self.visible_count.min(self.all_lines.len());
        self.all_lines.iter().take(end).map(|s| s.as_ref())
    }

    /// Push a new sentence and make it visible immediately.
    ///
    /// Called when the LLM emits a complete sentence. The sentence appears
    /// at the bottom of the text area. Auto-scrolls to keep the latest text
    /// in view, even if the user previously scrolled up — the meditation
    /// text should always follow the live generation. Drops the oldest
    /// pair of lines when the deque exceeds `MAX_LINES` to bound RAM.
    #[expect(clippy::cast_possible_truncation)] // line counts are small
    pub fn push_sentence(&mut self, sentence: String) {
        self.all_lines.push_back(sentence.into_boxed_str());
        // Add a blank line after each sentence/pause for vertical spacing.
        // This makes the text more spacious and meditative, and naturally
        // limits the visible content to ~5-8 items on the 40-row display.
        self.all_lines.push_back(Box::from(""));
        while self.all_lines.len() > MAX_LINES {
            self.all_lines.pop_front();
        }
        self.visible_count = self.all_lines.len();
        // Auto-scroll: keep the bottom of the text visible.
        if self.visible_count > 5 {
            self.scroll = (self.visible_count - 5) as u16;
        }
    }

    /// Transition to [`AppState::Generating`] and start the speed timer.
    pub fn start_generating(&mut self) {
        self.state = AppState::Generating;
        self.token_count = 0;
        self.generation_start = Some(Instant::now());
    }

    /// Transition to [`AppState::Paused`] with a countdown duration.
    /// Not yet used — will be activated when LLM outputs `[pause N]` markers.
    #[expect(dead_code)]
    pub fn start_pause(&mut self, seconds: f32) {
        self.state = AppState::Paused;
        self.pause_remaining = Some(seconds);
    }

    /// Transition to [`AppState::Done`].
    pub fn finish(&mut self) {
        self.state = AppState::Done;
        self.pause_remaining = None;
        self.generation_start = None;
    }

    /// Reset to [`AppState::Idle`], clearing all text and stats.
    pub fn reset(&mut self) {
        self.all_lines.clear();
        self.visible_count = 0;
        self.state = AppState::Idle;
        self.scroll = 0;
        self.token_count = 0;
        self.generation_start = None;
        self.pause_remaining = None;
    }

    /// Tokens per second since generation started. Returns 0.0 if not generating.
    #[allow(clippy::cast_precision_loss)] // token counts are small; f32 is fine
    pub fn tokens_per_sec(&self) -> f32 {
        self.generation_start.map_or(0.0, |start| {
            let elapsed = start.elapsed().as_secs_f32();
            if elapsed > 0.0 {
                self.token_count as f32 / elapsed
            } else {
                0.0
            }
        })
    }

    /// Scroll text up by one line. Clamped at the top.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll text down by one line.
    ///
    /// No upper clamp here — ratatui's [`Paragraph`] handles overflow
    /// gracefully by showing blank space past the end.
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }
}

/// Render the TUI layout to the given frame.
///
/// The layout is split into three vertical sections:
/// - **Header**: retro banner with zen motif
/// - **Body**: meditation text with pause markers styled (sentence reveal)
/// - **Footer**: state, stats, pause countdown, and button mappings
pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.theme;
    let bg = Style::default().bg(t.bg);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // body
            Constraint::Length(4), // footer
        ])
        .split(frame.area());

    // Fill background
    frame.render_widget(Block::default().style(bg), frame.area());

    // Header — retro zen banner
    let banner = vec![Line::from(vec![
        Span::styled(
            "༄  jhana-rs  ",
            Style::default()
                .fg(t.accent)
                .bg(t.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("～ breathe ～", Style::default().fg(t.title_dim).bg(t.bg)),
    ])];
    let header = Paragraph::new(banner).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(t.accent_dim).bg(t.bg)),
    );
    frame.render_widget(header, chunks[0]);

    // Body — meditation text with styled pause markers (sentence reveal)
    let text_lines: Vec<Line> = app
        .visible_lines()
        .iter()
        .map(|s| {
            if s.starts_with("[pause") || s.starts_with('[') && s.ends_with(']') {
                let inner = s.trim_start_matches('[').trim_end_matches(']');
                Line::from(Span::styled(
                    format!("  · · · {inner} · · ·"),
                    Style::default()
                        .fg(t.pause)
                        .bg(t.bg)
                        .add_modifier(Modifier::DIM),
                ))
            } else if s.is_empty() {
                Line::from("")
            } else {
                Line::from(Span::styled(
                    format!("  {s}"),
                    Style::default().fg(t.body).bg(t.bg),
                ))
            }
        })
        .collect();

    let body = Paragraph::new(text_lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(t.accent_dim).bg(t.bg))
                .title(Span::styled(
                    " meditation ",
                    Style::default().fg(t.status).bg(t.bg),
                )),
        );
    frame.render_widget(body, chunks[1]);

    // Footer — state, stats, pause countdown, and button mappings
    let status_spans = build_status_spans(app);
    let footer_lines = vec![
        Line::from(status_spans),
        Line::from(vec![
            Span::styled("  ←", Style::default().fg(t.accent).bg(t.bg)),
            Span::styled("quit ", Style::default().fg(t.accent_dim).bg(t.bg)),
            Span::styled("→", Style::default().fg(t.accent).bg(t.bg)),
            Span::styled("start ", Style::default().fg(t.accent_dim).bg(t.bg)),
            Span::styled("↑", Style::default().fg(t.accent).bg(t.bg)),
            Span::styled("up ", Style::default().fg(t.accent_dim).bg(t.bg)),
            Span::styled("↓", Style::default().fg(t.accent).bg(t.bg)),
            Span::styled("down ", Style::default().fg(t.accent_dim).bg(t.bg)),
            Span::styled("q", Style::default().fg(t.accent).bg(t.bg)),
            Span::styled(":quit", Style::default().fg(t.accent_dim).bg(t.bg)),
        ]),
    ];
    let footer = Paragraph::new(footer_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(t.accent_dim).bg(t.bg)),
    );
    frame.render_widget(footer, chunks[2]);
}

/// Build the status line spans based on current app state.
fn build_status_spans(app: &App) -> Vec<Span<'_>> {
    let t = &app.theme;
    let mut spans = vec![
        Span::styled("  ◈ ", Style::default().fg(t.accent).bg(t.bg)),
        Span::styled(
            app.state.to_string(),
            Style::default().fg(t.status).bg(t.bg),
        ),
    ];

    match app.state {
        AppState::Generating => {
            let tps = app.tokens_per_sec();
            spans.push(Span::styled(
                format!("  {} tok  {tps:.1} t/s", app.token_count),
                Style::default().fg(t.accent_dim).bg(t.bg),
            ));
        }
        AppState::Paused => {
            if let Some(remaining) = app.pause_remaining {
                spans.push(Span::styled(
                    format!("  · · · {remaining:.0}s · · ·"),
                    Style::default().fg(t.pause).bg(t.bg),
                ));
            }
        }
        AppState::Done => {
            spans.push(Span::styled(
                format!("  {} tok", app.token_count),
                Style::default().fg(t.accent_dim).bg(t.bg),
            ));
        }
        AppState::Idle => {}
    }

    spans
}
