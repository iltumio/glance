use super::matrix::{geometry, MatrixWidget};
use crate::app::App;
use crate::composer::{Composer, Focus};
use crate::widgets::CATALOG;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

struct Regions {
    title: Rect,
    preview: Rect,
    catalog: Rect,
    placed: Rect,
    footer: Rect,
}

fn regions(area: Rect) -> Option<Regions> {
    if area.width < 64 || area.height < 24 {
        return None;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(4),
    ])
    .split(area);
    let columns = Layout::horizontal([Constraint::Length(25), Constraint::Min(0)]).split(rows[1]);
    let right = Layout::vertical([Constraint::Length(12), Constraint::Min(0)]).split(columns[1]);
    Some(Regions {
        title: rows[0],
        preview: columns[0],
        catalog: right[0],
        placed: right[1],
        footer: rows[2],
    })
}

fn border(title: &str, active: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if active { Color::Cyan } else { Color::DarkGray }))
}

// Keep the selected row visible; rendering and mouse hit testing share the offset.
fn list_window(selected: usize, count: usize, rect: Rect) -> (usize, usize) {
    let rows = usize::from(rect.height.saturating_sub(2));
    let start = selected
        .saturating_add(1)
        .saturating_sub(rows)
        .min(count.saturating_sub(rows));
    (start, rows)
}

pub fn render(frame: &mut Frame, app: &App) {
    let composer = app.composer.as_ref().unwrap();
    let Some(regions) = regions(frame.area()) else {
        frame.render_widget(Paragraph::new("Terminal too small: widget editor needs 64x24. Resize or use --headless --template FILE. Ctrl-C: quit").wrap(Wrap { trim: false }), frame.area());
        return;
    };
    frame.render_widget(
        Paragraph::new(format!(
            "Widget Composer | 9x34 LEDs | {}{}",
            composer.template_path.display(),
            if composer.dirty { " *" } else { "" }
        ))
        .style(Style::default().fg(Color::Cyan)),
        regions.title,
    );
    let preview = border("Preview 9x34", composer.focus == Focus::Moving);
    let inner = preview.inner(regions.preview);
    frame.render_widget(preview, regions.preview);
    let grid = composer.runtime.frame();
    let highlight = if composer.focus == Focus::Playing {
        None
    } else {
        composer.selected_bounds()
    };
    frame.render_widget(MatrixWidget::new(&grid).highlight(highlight), inner);
    let (catalog_start, catalog_rows) =
        list_window(composer.catalog_index, CATALOG.len(), regions.catalog);
    let items: Vec<_> = CATALOG
        .iter()
        .enumerate()
        .skip(catalog_start)
        .take(catalog_rows)
        .map(|(index, kind)| {
            let size = kind.size();
            let available = composer.available(index);
            ListItem::new(format!(
                "{} {} {}x{}{}",
                if composer.focus == Focus::Catalog && composer.catalog_index == index {
                    ">"
                } else {
                    " "
                },
                kind.name(),
                size.width,
                size.height,
                if available { "" } else { " [no room]" }
            ))
            .style(Style::default().fg(if !available {
                Color::DarkGray
            } else if composer.focus == Focus::Catalog && composer.catalog_index == index {
                Color::Yellow
            } else {
                Color::White
            }))
        })
        .collect();
    frame.render_widget(
        List::new(items).block(border("Add widgets", composer.focus == Focus::Catalog)),
        regions.catalog,
    );
    let (placed_start, placed_rows) = list_window(
        composer.placed_index,
        composer.runtime.template().widgets.len(),
        regions.placed,
    );
    let items: Vec<_> = composer
        .runtime
        .template()
        .widgets
        .iter()
        .enumerate()
        .skip(placed_start)
        .take(placed_rows)
        .map(|(index, widget)| {
            ListItem::new(format!(
                "{} {} @ ({},{})",
                if composer.focus != Focus::Catalog && composer.placed_index == index {
                    ">"
                } else {
                    " "
                },
                widget.name.as_deref().unwrap_or(widget.kind.name()),
                widget.x,
                widget.y
            ))
            .style(Style::default().fg(
                if composer.focus != Focus::Catalog && composer.placed_index == index {
                    Color::Yellow
                } else {
                    Color::White
                },
            ))
        })
        .collect();
    frame.render_widget(
        List::new(items).block(border("Placed widgets", composer.focus != Focus::Catalog)),
        regions.placed,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{}\n{}\n{}\n{} | {}",
            composer.controls(),
            composer.detail(),
            composer.notice,
            app.device_status(),
            composer.api_status
        )),
        regions.footer,
    );
    if let Some(editor) = &composer.editor {
        let rect = Rect::new(
            regions.catalog.x,
            regions.catalog.y,
            regions.catalog.width,
            regions.catalog.height + regions.placed.height,
        );
        frame.render_widget(Clear, rect);
        let block = border("Configure Progress Bar", true);
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let value_width = usize::from(inner.width.saturating_sub(17));
        let mut lines: Vec<Line> = crate::widgets::progress_editor::LABELS
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let value = &editor.fields[index];
                let value: String = value
                    .chars()
                    .skip(value.chars().count().saturating_sub(value_width))
                    .collect();
                Line::from(format!(
                    "{} {:14} {}",
                    if editor.selected == index { ">" } else { " " },
                    label,
                    value
                ))
                .style(Style::default().fg(if editor.selected == index {
                    Color::Yellow
                } else {
                    Color::White
                }))
            })
            .collect();
        lines.push(Line::from("Up/Down: field | Del: clear | type to edit"));
        lines.push(Line::from("Enter: apply | Esc: cancel"));
        lines.push(Line::from(editor.error.clone()).style(Style::default().fg(Color::Red)));
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

pub fn handle_mouse(composer: &mut Composer, mouse: MouseEvent, area: Rect) {
    if composer.editor.is_some() {
        return;
    }
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
    ) {
        return;
    }
    let Some(regions) = regions(area) else {
        return;
    };
    let (x, y) = (mouse.column, mouse.row);
    if matches!(mouse.kind, MouseEventKind::Down(_)) {
        for (rect, catalog) in [(regions.catalog, true), (regions.placed, false)] {
            if x > rect.x
                && x < rect.right().saturating_sub(1)
                && y > rect.y
                && y < rect.bottom().saturating_sub(1)
            {
                let start = if catalog {
                    list_window(composer.catalog_index, CATALOG.len(), rect).0
                } else {
                    list_window(
                        composer.placed_index,
                        composer.runtime.template().widgets.len(),
                        rect,
                    )
                    .0
                };
                let index = start + usize::from(y - rect.y - 1);
                if catalog {
                    composer.choose_catalog(index);
                } else {
                    composer.choose_placed(index);
                }
                return;
            }
        }
    }
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(regions.preview);
    if let Some((rect, compact)) = geometry(inner) {
        if x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom() {
            let col = usize::from(x - rect.x) / if compact { 1 } else { 2 };
            let row = usize::from(y - rect.y) * if compact { 2 } else { 1 };
            composer.position_or_select(col, row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{Template, WidgetKind};
    use ratatui::{backend::TestBackend, Terminal};
    use std::time::Instant;
    #[test]
    fn catalog_keeps_last_widget_visible_and_mouse_targets_that_row() {
        let now = Instant::now();
        let mut app = App::new(true, None, now);
        app.enable_composer(Template::default(), "layout.json".into())
            .unwrap();
        app.composer.as_mut().unwrap().catalog_index = CATALOG.len() - 1;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("> Progress Bar 3x1"));
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 27,
            row: 11,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        let composer = app.composer.as_mut().unwrap();
        handle_mouse(composer, click, Rect::new(0, 0, 80, 24));
        assert_eq!(
            composer.runtime.template().widgets[0].kind,
            WidgetKind::ProgressBar
        );

        // A disabled row stays inert.
        composer.focus = Focus::Catalog;
        composer.runtime.remove(0);
        composer.runtime.add(WidgetKind::Snake).unwrap();
        handle_mouse(composer, click, Rect::new(0, 0, 80, 24));
        assert_eq!(composer.runtime.template().widgets.len(), 1);
        assert_eq!(
            composer.runtime.template().widgets[0].kind,
            WidgetKind::Snake
        );
    }

    #[test]
    fn editor_shows_preview_left_and_disabled_catalog_rows_are_gray_and_inert() {
        let now = Instant::now();
        let mut app = App::new(true, None, now);
        app.enable_composer(Template::default(), "layout.json".into())
            .unwrap();
        app.composer
            .as_mut()
            .unwrap()
            .runtime
            .add(WidgetKind::SevenSegmentClock)
            .unwrap();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(26, 2)].fg, Color::DarkGray);
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("Preview 9x34") && text.contains("[no room]"));
        handle_mouse(
            app.composer.as_mut().unwrap(),
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 27,
                row: 2,
                modifiers: crossterm::event::KeyModifiers::NONE,
            },
            Rect::new(0, 0, 80, 24),
        );
        assert_eq!(
            app.composer
                .as_ref()
                .unwrap()
                .runtime
                .template()
                .widgets
                .len(),
            1
        );
    }
}
