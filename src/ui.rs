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
//! │          ༄                      │
//! │    ～ breathe ～                 │
//! ├─────────────────────────────────┤
//! │                                 │
//! │  Close your eyes and take a     │
//! │  deep breath in.                │
//! │                                 │
//! │  · · · 5s · · ·                 │
//! │                                 │
//! ├─────────────────────────────────┤
//! │  ◈ Demo          q: quit        │
//! └─────────────────────────────────┘
//! ```

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

/// Application state displayed in the TUI.
///
/// Holds the meditation text, scroll position, and status label.
/// Scroll offset exists because the Rock's 720x1280 display at 32px font
/// height gives only ~34 visible body rows — longer meditations need scrolling.
pub struct App {
    /// Lines of meditation text to display.
    pub lines: Vec<String>,
    /// Current application state label (e.g. "Idle", "Generating", "Demo").
    pub state: String,
    /// Vertical scroll offset into the text (0 = top).
    pub scroll: u16,
}

impl App {
    /// Create a new [`App`] with empty text and "Idle" state.
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            state: String::from("Idle"),
            scroll: 0,
        }
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
/// - **Body**: meditation text with pause markers styled
/// - **Footer**: status bar with keybindings
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

    // Body — meditation text with styled pause markers
    let text_lines: Vec<Line> = app
        .lines
        .iter()
        .map(|s| {
            if s.starts_with("[pause") || s.starts_with('[') && s.ends_with(']') {
                // Render pause markers in a distinct style
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

    // Footer — status + button/key mappings
    let footer_lines = vec![
        Line::from(vec![
            Span::styled("  ◈ ", Style::default().fg(palette::AMBER)),
            Span::styled(&app.state, Style::default().fg(palette::GREEN)),
        ]),
        Line::from(vec![
            Span::styled("  ▲", Style::default().fg(palette::AMBER)),
            Span::styled("up ", Style::default().fg(palette::DIM_GREEN)),
            Span::styled("▼", Style::default().fg(palette::AMBER)),
            Span::styled("down ", Style::default().fg(palette::DIM_GREEN)),
            Span::styled("●", Style::default().fg(palette::AMBER)),
            Span::styled("start ", Style::default().fg(palette::DIM_GREEN)),
            Span::styled("◀", Style::default().fg(palette::AMBER)),
            Span::styled("quit ", Style::default().fg(palette::DIM_GREEN)),
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
