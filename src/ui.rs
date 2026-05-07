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

use std::time::Instant;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Retro color palette — soft phosphor greens and amber on dark background.
///
/// RGB colors are used instead of terminal palette indices because the
/// Rock's `TERM=linux` console supports 24-bit color via DRM/KMS, and
/// fixed RGB values ensure consistent appearance regardless of terminal
/// theme.
mod palette {
    use ratatui::style::Color;

    /// Phosphor green — main text.
    pub const GREEN: Color = Color::Rgb(80, 220, 120);
    /// Dim green — secondary text, borders.
    pub const DIM_GREEN: Color = Color::Rgb(40, 110, 60);
    /// Amber — highlights, titles.
    pub const AMBER: Color = Color::Rgb(255, 176, 0);
    /// Dim amber — status text.
    pub const DIM_AMBER: Color = Color::Rgb(160, 110, 0);
    /// Soft white — meditation body text.
    pub const SOFT_WHITE: Color = Color::Rgb(200, 200, 180);
    /// Pause marker color.
    pub const PAUSE: Color = Color::Rgb(120, 100, 180);
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
/// Holds the meditation text, scroll position, generation stats, and
/// lifecycle state. Scroll offset exists because the Rock's 720x1280
/// display at 32px font height gives only ~34 visible body rows —
/// longer meditations need scrolling.
pub struct App {
    /// All generated lines (the full meditation text so far).
    /// New lines are appended as the LLM streams them.
    all_lines: Vec<String>,
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
}

impl App {
    /// Create a new [`App`] in [`AppState::Idle`].
    pub fn new() -> Self {
        Self {
            all_lines: Vec::new(),
            visible_count: 0,
            state: AppState::Idle,
            scroll: 0,
            token_count: 0,
            generation_start: None,
            pause_remaining: None,
        }
    }

    /// Lines currently visible in the TUI (for sentence-by-sentence reveal).
    pub fn visible_lines(&self) -> &[String] {
        let end = self.visible_count.min(self.all_lines.len());
        &self.all_lines[..end]
    }

    /// Push a new sentence and make it visible immediately.
    ///
    /// Called when the LLM emits a complete sentence. The sentence appears
    /// at the bottom of the text area and auto-scrolls to keep it in view.
    /// Not yet used — will be called from LLM streaming integration.
    #[expect(dead_code)]
    pub fn push_sentence(&mut self, sentence: String) {
        self.all_lines.push(sentence);
        self.visible_count = self.all_lines.len();
    }

    /// Push a line without revealing it yet.
    ///
    /// Used for preloading demo text. Call [`reveal_next`] to show lines
    /// one at a time.
    pub fn push_hidden(&mut self, line: String) {
        self.all_lines.push(line);
    }

    /// Reveal the next hidden line. Returns `true` if a line was revealed.
    pub fn reveal_next(&mut self) -> bool {
        if self.visible_count < self.all_lines.len() {
            self.visible_count += 1;
            true
        } else {
            false
        }
    }

    /// Reveal all remaining hidden lines at once.
    /// Not yet used — will be called when user skips ahead during generation.
    #[expect(dead_code)]
    pub fn reveal_all(&mut self) {
        self.visible_count = self.all_lines.len();
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // body
            Constraint::Length(4), // footer
        ])
        .split(frame.area());

    // Header — retro zen banner
    let banner = vec![Line::from(vec![
        Span::styled(
            "༄  jhana-rs  ",
            Style::default()
                .fg(palette::AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("～ breathe ～", Style::default().fg(palette::DIM_AMBER)),
    ])];
    let header = Paragraph::new(banner).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::DIM_GREEN)),
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
                        .fg(palette::PAUSE)
                        .add_modifier(Modifier::DIM),
                ))
            } else if s.is_empty() {
                Line::from("")
            } else {
                Line::from(Span::styled(
                    format!("  {s}"),
                    Style::default().fg(palette::SOFT_WHITE),
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
                .border_style(Style::default().fg(palette::DIM_GREEN))
                .title(Span::styled(
                    " meditation ",
                    Style::default().fg(palette::GREEN),
                )),
        );
    frame.render_widget(body, chunks[1]);

    // Footer — state, stats, pause countdown, and button mappings
    let status_spans = build_status_spans(app);
    let footer_lines = vec![
        Line::from(status_spans),
        Line::from(vec![
            Span::styled("  ←", Style::default().fg(palette::AMBER)),
            Span::styled("quit ", Style::default().fg(palette::DIM_GREEN)),
            Span::styled("→", Style::default().fg(palette::AMBER)),
            Span::styled("start ", Style::default().fg(palette::DIM_GREEN)),
            Span::styled("↑", Style::default().fg(palette::AMBER)),
            Span::styled("up ", Style::default().fg(palette::DIM_GREEN)),
            Span::styled("↓", Style::default().fg(palette::AMBER)),
            Span::styled("down ", Style::default().fg(palette::DIM_GREEN)),
            Span::styled("q", Style::default().fg(palette::AMBER)),
            Span::styled(":quit", Style::default().fg(palette::DIM_GREEN)),
        ]),
    ];
    let footer = Paragraph::new(footer_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::DIM_GREEN)),
    );
    frame.render_widget(footer, chunks[2]);
}

/// Build the status line spans based on current app state.
///
/// Shows different information depending on lifecycle state:
/// - **Idle**: just the state name
/// - **Generating**: state + token count + tokens/sec
/// - **Paused**: state + countdown
/// - **Done**: state + final token count
fn build_status_spans(app: &App) -> Vec<Span<'_>> {
    let mut spans = vec![
        Span::styled("  ◈ ", Style::default().fg(palette::AMBER)),
        Span::styled(app.state.to_string(), Style::default().fg(palette::GREEN)),
    ];

    match app.state {
        AppState::Generating => {
            let tps = app.tokens_per_sec();
            spans.push(Span::styled(
                format!("  {} tok  {tps:.1} t/s", app.token_count),
                Style::default().fg(palette::DIM_GREEN),
            ));
        }
        AppState::Paused => {
            if let Some(remaining) = app.pause_remaining {
                spans.push(Span::styled(
                    format!("  · · · {remaining:.0}s · · ·"),
                    Style::default().fg(palette::PAUSE),
                ));
            }
        }
        AppState::Done => {
            spans.push(Span::styled(
                format!("  {} tok", app.token_count),
                Style::default().fg(palette::DIM_GREEN),
            ));
        }
        AppState::Idle => {}
    }

    spans
}
