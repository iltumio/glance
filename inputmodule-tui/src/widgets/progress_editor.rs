use super::{
    progress::{Orientation, ProgressConfig},
    template::invalid,
    Placement,
};
use crate::inputs::{FileMask, InputSource};
use crossterm::event::KeyCode;
use std::io;

pub const LABELS: [&str; 14] = [
    "Name",
    "Orientation",
    "Length (LEDs)",
    "Width (LEDs)",
    "Minimum",
    "Maximum",
    "Initial value",
    "Reverse",
    "Input",
    "File path",
    "Interval (ms)",
    "Mask type",
    "Mask",
    "Regex group",
];
pub struct ProgressEditor {
    pub fields: Vec<String>,
    pub selected: usize,
    pub error: String,
    original: Placement,
}
impl ProgressEditor {
    pub fn new(placement: &Placement) -> Self {
        let config = placement.progress.clone().unwrap_or_default();
        let (source, path, interval, mask) = match &placement.input {
            None => ("manual", String::new(), 1000, FileMask::Number),
            Some(InputSource::Api) => ("api", String::new(), 1000, FileMask::Number),
            Some(InputSource::File {
                path,
                interval_ms,
                mask,
            }) => (
                "file",
                path.to_string_lossy().into_owned(),
                *interval_ms,
                mask.clone(),
            ),
        };
        let (mask_type, mask_text, group) = match mask {
            FileMask::Number => ("number", String::new(), 1),
            FileMask::JsonPointer { pointer } => ("json_pointer", pointer, 1),
            FileMask::Regex { pattern, group } => ("regex", pattern, group),
        };
        Self {
            fields: vec![
                placement.name.clone().unwrap_or_default(),
                match config.orientation {
                    Orientation::Horizontal => "horizontal",
                    Orientation::Vertical => "vertical",
                }
                .into(),
                config.length.to_string(),
                config.width.to_string(),
                config.min.to_string(),
                config.max.to_string(),
                config.value.to_string(),
                config.reverse.to_string(),
                source.into(),
                path,
                interval.to_string(),
                mask_type.into(),
                mask_text,
                group.to_string(),
            ],
            selected: 0,
            error: String::new(),
            original: placement.clone(),
        }
    }
    pub fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Up | KeyCode::BackTab => {
                self.selected = (self.selected + LABELS.len() - 1) % LABELS.len()
            }
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1) % LABELS.len(),
            KeyCode::Backspace => {
                self.fields[self.selected].pop();
            }
            KeyCode::Delete => self.fields[self.selected].clear(),
            KeyCode::Char(c) if self.fields[self.selected].len() < 4096 => {
                self.fields[self.selected].push(c);
            }
            _ => {}
        }
    }
    pub fn placement(&self) -> io::Result<Placement> {
        let f = &self.fields;
        let number = |index: usize| {
            f[index]
                .parse::<f64>()
                .map_err(|_| invalid(format!("{} must be a number", LABELS[index])))
        };
        let integer = |index: usize| {
            f[index]
                .parse::<usize>()
                .map_err(|_| invalid(format!("{} must be a positive integer", LABELS[index])))
        };
        let mut placement = self.original.clone();
        placement.name = (!f[0].is_empty()).then(|| f[0].clone());
        placement.progress = Some(ProgressConfig {
            orientation: match f[1].as_str() {
                "horizontal" => Orientation::Horizontal,
                "vertical" => Orientation::Vertical,
                _ => return Err(invalid("Orientation: horizontal or vertical")),
            },
            length: integer(2)?,
            width: integer(3)?,
            min: number(4)?,
            max: number(5)?,
            value: number(6)?,
            reverse: f[7]
                .parse()
                .map_err(|_| invalid("Reverse: true or false"))?,
        });
        placement.input = match f[8].as_str() {
            "manual" => None,
            "api" => Some(InputSource::Api),
            "file" => Some(InputSource::File {
                path: f[9].clone().into(),
                interval_ms: f[10]
                    .parse()
                    .map_err(|_| invalid("Interval must be an integer"))?,
                mask: match f[11].as_str() {
                    "number" => FileMask::Number,
                    "json_pointer" => FileMask::JsonPointer {
                        pointer: f[12].clone(),
                    },
                    "regex" => FileMask::Regex {
                        pattern: f[12].clone(),
                        group: integer(13)?,
                    },
                    _ => return Err(invalid("Mask type: number, json_pointer or regex")),
                },
            }),
            _ => return Err(invalid("Input: manual, file or api")),
        };
        Ok(placement)
    }
}
