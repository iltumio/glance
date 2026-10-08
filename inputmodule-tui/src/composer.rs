use crate::widgets::progress_editor::ProgressEditor;
use crate::widgets::{Bounds, Runtime, Template, WidgetKind, CATALOG};
use crossterm::event::KeyCode;
use std::io;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Catalog,
    Placed,
    Moving,
    Playing,
}

pub struct Composer {
    pub runtime: Runtime,
    pub focus: Focus,
    pub catalog_index: usize,
    pub placed_index: usize,
    pub template_path: PathBuf,
    pub notice: String,
    pub dirty: bool,
    move_origin: Option<(usize, usize)>,
    pub editor: Option<ProgressEditor>,
    pub api_status: String,
}

impl Composer {
    pub fn new(template: Template, template_path: PathBuf, now: Instant) -> io::Result<Self> {
        Ok(Self {
            runtime: Runtime::with_base(
                template,
                now,
                template_path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(std::path::Path::new(".")),
            )?,
            focus: Focus::Catalog,
            catalog_index: 0,
            placed_index: 0,
            template_path,
            notice: "Select a widget and press Enter to add it".into(),
            dirty: false,
            move_origin: None,
            editor: None,
            api_status: String::new(),
        })
    }
    pub fn selected_bounds(&self) -> Option<Bounds> {
        if self.focus == Focus::Catalog {
            return None;
        }
        self.runtime
            .template()
            .widgets
            .get(self.placed_index)
            .map(|widget| widget.bounds())
    }
    pub fn playing(&self) -> Option<usize> {
        (self.focus == Focus::Playing).then_some(self.placed_index)
    }
    pub fn available(&self, index: usize) -> bool {
        CATALOG
            .get(index)
            .is_some_and(|&kind| self.runtime.template().first_slot(kind).is_some())
    }
    pub fn add_selected(&mut self) {
        match self.runtime.add(CATALOG[self.catalog_index]) {
            Ok(index) => {
                self.placed_index = index;
                self.focus = Focus::Moving;
                self.move_origin = None;
                self.dirty = true;
                self.notice = "Move with arrows, then Enter to place; Esc cancels".into();
            }
            Err(error) => self.notice = error.to_string(),
        }
    }
    pub fn choose_catalog(&mut self, index: usize) {
        if self.editor.is_some()
            || index >= CATALOG.len()
            || matches!(self.focus, Focus::Moving | Focus::Playing)
        {
            return;
        }
        if !self.available(index) {
            return;
        }
        self.catalog_index = index;
        self.focus = Focus::Catalog;
        // Disabled catalog rows cannot be activated with the mouse either.
        self.add_selected();
    }
    pub fn choose_placed(&mut self, index: usize) {
        if self.editor.is_none()
            && index < self.runtime.template().widgets.len()
            && !matches!(self.focus, Focus::Moving | Focus::Playing)
        {
            self.placed_index = index;
            self.focus = Focus::Placed;
        }
    }
    pub fn position_or_select(&mut self, x: usize, y: usize) {
        if self.editor.is_some() {
            return;
        }
        if self.focus == Focus::Moving {
            self.move_to(x, y);
        } else if self.focus != Focus::Playing {
            let index = self
                .runtime
                .template()
                .widgets
                .iter()
                .position(|widget| widget.bounds().contains(x, y));
            if let Some(index) = index {
                self.choose_placed(index);
            }
        }
    }
    fn move_to(&mut self, x: usize, y: usize) {
        if self.runtime.move_to(self.placed_index, x, y) {
            self.dirty = true;
            self.notice = format!("Position: ({x}, {y}) | Enter to place");
        } else {
            self.notice = "Position blocked: outside canvas or overlapping another widget".into();
        }
    }
    fn cancel_move(&mut self) {
        if let Some((x, y)) = self.move_origin.take() {
            self.runtime.move_to(self.placed_index, x, y);
        } else {
            self.runtime.remove(self.placed_index);
        }
        self.placed_index = self
            .placed_index
            .min(self.runtime.template().widgets.len().saturating_sub(1));
        self.focus = if self.runtime.template().widgets.is_empty() {
            Focus::Catalog
        } else {
            Focus::Placed
        };
        self.notice = "Placement cancelled".into();
    }
    fn save(&mut self) {
        if self.focus == Focus::Moving {
            self.focus = Focus::Placed;
            self.move_origin = None;
        }
        match self.runtime.template().save(&self.template_path) {
            Ok(()) => {
                self.dirty = false;
                self.notice = format!("Saved {}", self.template_path.display());
            }
            Err(error) => self.notice = format!("Save failed: {error}"),
        }
    }
    /// Returns true only when the user requests exit from the editor.
    pub fn handle_key(&mut self, key: KeyCode, now: Instant) -> bool {
        if let Some(editor) = &mut self.editor {
            match key {
                KeyCode::Esc => {
                    self.editor = None;
                    self.notice = "Configuration cancelled".into();
                }
                KeyCode::Enter => {
                    let result = editor
                        .placement()
                        .and_then(|placement| self.runtime.configure(self.placed_index, placement));
                    match result {
                        Ok(()) => {
                            self.editor = None;
                            self.dirty = true;
                            self.notice = "Widget configured".into();
                        }
                        Err(error) => editor.error = error.to_string(),
                    }
                }
                _ => editor.handle_key(key),
            }
            return false;
        }
        if self.focus == Focus::Playing {
            if key == KeyCode::Esc {
                self.focus = Focus::Placed;
                return false;
            }
            let kind = self.runtime.template().widgets[self.placed_index].kind;
            if key == KeyCode::Char('r') && kind != WidgetKind::Wpm {
                self.runtime.restart(self.placed_index);
            } else {
                self.runtime.handle_key(self.placed_index, key, now);
                if kind == WidgetKind::Life && key == KeyCode::Char('n') {
                    self.dirty = true;
                }
            }
            return false;
        }
        if key == KeyCode::Char('s') {
            self.save();
            return false;
        }
        if self.focus == Focus::Moving {
            match key {
                KeyCode::Enter => {
                    self.focus = Focus::Placed;
                    self.move_origin = None;
                    self.notice = "Widget placed".into();
                }
                KeyCode::Esc => self.cancel_move(),
                _ => {
                    let delta = match key {
                        KeyCode::Left | KeyCode::Char('h') => Some((-1, 0)),
                        KeyCode::Right | KeyCode::Char('l') => Some((1, 0)),
                        KeyCode::Up | KeyCode::Char('k') => Some((0, -1)),
                        KeyCode::Down | KeyCode::Char('j') => Some((0, 1)),
                        _ => None,
                    };
                    if let Some((dx, dy)) = delta {
                        let widget = &self.runtime.template().widgets[self.placed_index];
                        if let (Some(x), Some(y)) = (
                            widget.x.checked_add_signed(dx),
                            widget.y.checked_add_signed(dy),
                        ) {
                            self.move_to(x, y);
                        } else {
                            self.notice = "Position blocked: outside canvas".into();
                        }
                    }
                }
            }
            return false;
        }
        match key {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus =
                    if self.focus == Focus::Catalog && !self.runtime.template().widgets.is_empty() {
                        Focus::Placed
                    } else {
                        Focus::Catalog
                    }
            }
            KeyCode::Up | KeyCode::Down => {
                let (index, count) = if self.focus == Focus::Catalog {
                    (&mut self.catalog_index, CATALOG.len())
                } else {
                    (
                        &mut self.placed_index,
                        self.runtime.template().widgets.len(),
                    )
                };
                if count > 0 {
                    *index = if key == KeyCode::Up {
                        (*index + count - 1) % count
                    } else {
                        (*index + 1) % count
                    };
                }
            }
            KeyCode::Enter if self.focus == Focus::Catalog => self.add_selected(),
            KeyCode::Enter => {
                if let Some(widget) = self.runtime.template().widgets.get(self.placed_index) {
                    self.move_origin = Some((widget.x, widget.y));
                    self.focus = Focus::Moving;
                    self.notice =
                        "Move with arrows; Enter confirms, Esc restores the position".into();
                }
            }
            KeyCode::Delete | KeyCode::Char('d') if self.focus == Focus::Placed => {
                self.runtime.remove(self.placed_index);
                self.placed_index = self
                    .placed_index
                    .min(self.runtime.template().widgets.len().saturating_sub(1));
                self.dirty = true;
                self.notice = "Widget removed".into();
                if self.runtime.template().widgets.is_empty() {
                    self.focus = Focus::Catalog;
                }
            }
            KeyCode::Char('e') if self.focus == Focus::Placed => {
                if let Some(widget) = self.runtime.template().widgets.get(self.placed_index) {
                    if widget.kind == WidgetKind::ProgressBar {
                        self.editor = Some(ProgressEditor::new(widget));
                    } else {
                        self.notice =
                            "Only Progress Bar currently has external input settings".into();
                    }
                }
            }
            KeyCode::Char('p') if self.focus == Focus::Placed => {
                if let Some(widget) = self.runtime.template().widgets.get(self.placed_index) {
                    if matches!(widget.kind, WidgetKind::Snake | WidgetKind::Ledris) {
                        self.runtime.restart(self.placed_index);
                    }
                    self.focus = Focus::Playing;
                    self.notice = "Widget input active | Esc returns to editing".into();
                }
            }
            _ => {}
        }
        false
    }
    pub fn controls(&self) -> &'static str {
        match self.focus {
            Focus::Catalog => "Arrows: select | Enter/click: add | Tab: placed | s: save | q: quit",
            Focus::Placed => {
                "Arrows: select | Enter: move | e: config | p: input | d: remove | Tab | s: save"
            }
            Focus::Moving => {
                "Arrows/hjkl: move | Enter: place | Esc: cancel | click preview: position"
            }
            Focus::Playing => {
                "Widget input active | r: restart (except WPM) | Esc: edit | Ctrl-C: quit"
            }
        }
    }
    pub fn detail(&self) -> String {
        if self.focus == Focus::Catalog {
            let kind = CATALOG[self.catalog_index];
            let size = kind.size();
            format!(
                "{} requires {}x{} | {} LEDs free{}",
                kind.name(),
                size.width,
                size.height,
                self.runtime.template().free_pixels(),
                if self.available(self.catalog_index) {
                    ""
                } else {
                    " | No free rectangle fits"
                }
            )
        } else {
            self.runtime.status(self.placed_index)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_editor_rejects_invalid_changes_cancels_and_saves_valid_input_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("layout.json");
        let now = Instant::now();
        let mut composer = Composer::new(Template::default(), path.clone(), now).unwrap();
        composer.catalog_index = CATALOG.len() - 1;
        composer.handle_key(KeyCode::Enter, now);
        composer.handle_key(KeyCode::Enter, now);
        let original = composer.runtime.template().clone();
        composer.handle_key(KeyCode::Char('e'), now);
        composer.editor.as_mut().unwrap().fields[2] = "2".into();
        composer.handle_key(KeyCode::Enter, now);
        assert!(composer
            .editor
            .as_ref()
            .unwrap()
            .error
            .contains("length >= 3"));
        assert_eq!(composer.runtime.template(), &original);
        composer.handle_key(KeyCode::Esc, now);
        assert!(composer.editor.is_none());
        composer.handle_key(KeyCode::Char('e'), now);
        let fields = &mut composer.editor.as_mut().unwrap().fields;
        fields[0] = "download".into();
        fields[2] = "9".into();
        fields[3] = "2".into();
        fields[8] = "api".into();
        composer.handle_key(KeyCode::Enter, now);
        assert!(composer.editor.is_none());
        composer.handle_key(KeyCode::Char('s'), now);
        let saved = Template::load(&path).unwrap();
        assert_eq!(
            saved.widgets[0].size(),
            crate::widgets::Size {
                width: 9,
                height: 2
            }
        );
        assert_eq!(
            saved.widgets[0].input,
            Some(crate::inputs::InputSource::Api)
        );
        assert_eq!(saved.widgets[0].name.as_deref(), Some("download"));
    }

    #[test]
    fn disabled_rows_reject_keyboard_and_mouse_and_removal_reenables_them() {
        let now = Instant::now();
        let mut composer = Composer::new(Template::default(), "unused.json".into(), now).unwrap();
        composer.catalog_index = 6;
        composer.handle_key(KeyCode::Enter, now);
        composer.handle_key(KeyCode::Enter, now);
        composer.handle_key(KeyCode::Tab, now);
        assert!(!composer.available(0));
        composer.choose_catalog(0);
        assert_eq!(composer.runtime.template().widgets.len(), 1);
        assert_eq!(composer.catalog_index, 6);
        composer.catalog_index = 0;
        composer.handle_key(KeyCode::Enter, now);
        assert_eq!(composer.runtime.template().widgets.len(), 1);
        composer.handle_key(KeyCode::Tab, now);
        composer.handle_key(KeyCode::Delete, now);
        assert!(composer.available(0));
    }
    #[test]
    fn new_move_can_cancel_and_existing_move_restores_original_position() {
        let now = Instant::now();
        let mut composer = Composer::new(Template::default(), "unused.json".into(), now).unwrap();
        composer.catalog_index = 6;
        composer.handle_key(KeyCode::Enter, now);
        composer.handle_key(KeyCode::Esc, now);
        assert!(composer.runtime.template().widgets.is_empty());
        composer.handle_key(KeyCode::Enter, now);
        composer.handle_key(KeyCode::Enter, now);
        composer.handle_key(KeyCode::Enter, now);
        composer.handle_key(KeyCode::Right, now);
        assert_eq!(composer.runtime.template().widgets[0].x, 1);
        composer.handle_key(KeyCode::Esc, now);
        assert_eq!(composer.runtime.template().widgets[0].x, 0);
    }
}
