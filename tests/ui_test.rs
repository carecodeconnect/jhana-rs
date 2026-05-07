//! Integration tests for the ratatui TUI layout.

use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

// Re-export from the binary crate isn't possible, so we test the UI
// module by duplicating the minimal public API. For proper integration
// testing, the UI module should be moved to a library crate.
//
// For now, we test that ratatui renders without panicking and produces
// expected content in the terminal buffer.

/// Verify that a basic ratatui terminal can be created and drawn to.
#[test]
fn terminal_renders_without_panic() {
    let backend = TestBackend::new(45, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            use ratatui::{
                layout::{Constraint, Direction, Layout},
                style::Style,
                widgets::{Block, Borders, Paragraph},
            };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(frame.area());

            let header = Paragraph::new("jhana-rs").block(Block::default().borders(Borders::ALL));
            frame.render_widget(header, chunks[0]);

            let body = Paragraph::new("Close your eyes and take a deep breath in.")
                .block(Block::default().borders(Borders::ALL).title(" meditation "));
            frame.render_widget(body, chunks[1]);

            let footer = Paragraph::new("  State: Demo")
                .style(Style::default())
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(footer, chunks[2]);
        })
        .unwrap();
}

/// Verify the rendered buffer contains expected text.
#[test]
fn buffer_contains_title_and_body() {
    let backend = TestBackend::new(45, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            use ratatui::{
                layout::{Constraint, Direction, Layout},
                widgets::{Block, Borders, Paragraph},
            };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(frame.area());

            let header = Paragraph::new("jhana-rs").block(Block::default().borders(Borders::ALL));
            frame.render_widget(header, chunks[0]);

            let body =
                Paragraph::new("breathe deeply").block(Block::default().borders(Borders::ALL));
            frame.render_widget(body, chunks[1]);

            let footer = Paragraph::new("Demo").block(Block::default().borders(Borders::ALL));
            frame.render_widget(footer, chunks[2]);
        })
        .unwrap();

    let buf: &Buffer = terminal.backend().buffer();
    let content = buffer_to_string(buf);

    assert!(content.contains("jhana-rs"), "buffer should contain title");
    assert!(
        content.contains("breathe deeply"),
        "buffer should contain body text"
    );
    assert!(content.contains("Demo"), "buffer should contain status");
}

/// Helper: flatten a ratatui buffer into a single string for assertions.
fn buffer_to_string(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::new();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            out.push_str(buf.cell((x, y)).map_or(" ", |c| c.symbol()));
        }
        out.push('\n');
    }
    out
}
