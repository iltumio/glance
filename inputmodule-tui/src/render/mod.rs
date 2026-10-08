pub mod composer;
pub mod matrix;
use crate::app::{App, AppState, MENU_ITEMS};
use crate::features::FeatureKind;
use crate::games::{game_of_life::PATTERNS, GameKind};
use crate::serial::protocol::GameId;
use matrix::MatrixWidget;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn render(frame: &mut Frame, app: &App) {
    if app.composer.is_some() {
        composer::render(frame, app);
        return;
    }
    let area = frame.area();
    let grid = app.display_grid();
    if grid.is_some() && (area.width < 20 || area.height < 23) {
        frame.render_widget(
            Paragraph::new(format!(
                "Terminal too small: {}x{}. Resize to at least 20x23. Esc: menu | Ctrl-C: quit",
                area.width, area.height
            ))
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .split(area);
    let (title, controls, detail) = match &app.state {
        AppState::Menu { .. } => (
            "Framework Input Module TUI".into(),
            "↑↓: select | Enter: start | q: quit",
            "".into(),
        ),
        AppState::Playing { game, .. } => (
            format!("{} | Score: {}", game.name(), game.score()),
            if game.kind() == GameKind::Ledris {
                "←→/hl: move | ↑/k: rotate | ↓/j: drop | Space: hard drop | q: menu"
            } else {
                "Arrows/hjkl: move | q: menu"
            },
            "".into(),
        ),
        AppState::GameOver { game, won, .. } => (
            format!(
                "{} | {} | Score: {}",
                game.name(),
                if *won { "YOU WIN" } else { "GAME OVER" },
                game.score()
            ),
            "r: restart | q/Esc: menu",
            "".into(),
        ),
        AppState::Feature { feature, .. } => (
            feature.name().into(),
            if feature.kind() == FeatureKind::Wpm {
                "Type to measure | Esc: menu"
            } else {
                "q/Esc: menu"
            },
            feature.status_text(),
        ),
        AppState::EmbeddedGame { game } => (
            format!(
                "{} | running on device",
                if *game == GameId::Pong {
                    "Pong"
                } else {
                    "Game of Life"
                }
            ),
            if *game == GameId::Pong {
                "←→: paddle 1 | a/d: paddle 2 | q: stop"
            } else {
                "q/Esc: stop"
            },
            "Watch the physical LED Matrix".into(),
        ),
        AppState::EmbeddedGameOfLife { .. } => (
            "Game of Life | starting pattern".into(),
            "↑↓: select | Enter: start | q: menu",
            "".into(),
        ),
    };
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(Color::Cyan)),
        chunks[0],
    );
    let block = Block::default().borders(Borders::ALL);
    if let Some(grid) = grid {
        let inner = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);
        frame.render_widget(MatrixWidget::new(&grid), inner);
    } else {
        match &app.state {
            AppState::Menu { selected } => {
                let items: Vec<_> = MENU_ITEMS
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        ListItem::new(format!(
                            "{} {}",
                            if i == *selected { ">" } else { " " },
                            item.label()
                        ))
                        .style(Style::default().fg(if i == *selected {
                            Color::Yellow
                        } else {
                            Color::White
                        }))
                    })
                    .collect();
                frame.render_widget(List::new(items).block(block), chunks[1]);
            }
            AppState::EmbeddedGameOfLife { selected_pattern } => {
                let items: Vec<_> = PATTERNS
                    .iter()
                    .enumerate()
                    .map(|(i, (_, name))| {
                        ListItem::new(format!(
                            "{} {}",
                            if i == *selected_pattern { ">" } else { " " },
                            name
                        ))
                        .style(Style::default().fg(
                            if i == *selected_pattern {
                                Color::Yellow
                            } else {
                                Color::White
                            },
                        ))
                    })
                    .collect();
                frame.render_widget(List::new(items).block(block), chunks[1]);
            }
            _ => frame.render_widget(
                Paragraph::new(detail.clone())
                    .block(block)
                    .wrap(Wrap { trim: false }),
                chunks[1],
            ),
        }
    }
    frame.render_widget(Paragraph::new(controls), chunks[2]);
    frame.render_widget(
        Paragraph::new(format!("{}\n{}", detail, app.device_status())).style(Style::default().fg(
            if app.error.is_some() {
                Color::Yellow
            } else {
                Color::DarkGray
            },
        )),
        chunks[3],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};
    use std::time::Instant;
    #[test]
    fn game_fits_standard_terminal_and_small_terminal_shows_warning() {
        let now = Instant::now();
        let mut app = App::new(true, None, now);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        for (w, h, warning) in [
            (80, 24, false),
            (20, 23, false),
            (80, 40, false),
            (19, 22, true),
            (0, 0, false),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| render(f, &app)).unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            if warning {
                assert!(text.contains("Terminal too small"));
            } else if w > 0 {
                assert!(text.contains("Snake"));
            }
        }
    }
}
