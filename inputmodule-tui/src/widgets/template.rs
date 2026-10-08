use super::progress::ProgressConfig;
use super::{LifePattern, WidgetKind};
use crate::games::{HEIGHT, WIDTH};
use crate::inputs::InputSource;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub x: usize,
    pub y: usize,
    pub size: Size,
}

impl Bounds {
    pub fn fits_canvas(self) -> bool {
        self.x <= WIDTH
            && self.y <= HEIGHT
            && self.size.width <= WIDTH - self.x
            && self.size.height <= HEIGHT - self.y
    }

    pub fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.size.width
            && other.x < self.x + self.size.width
            && self.y < other.y + other.size.height
            && other.y < self.y + self.size.height
    }

    pub fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x && x - self.x < self.size.width && y >= self.y && y - self.y < self.size.height
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub kind: WidgetKind,
    pub x: usize,
    pub y: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<LifePattern>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSource>,
}

impl Placement {
    pub fn new(kind: WidgetKind, x: usize, y: usize) -> Self {
        Self {
            kind,
            x,
            y,
            pattern: None,
            name: None,
            progress: None,
            input: None,
        }
    }
    pub fn size(&self) -> Size {
        if self.kind == WidgetKind::ProgressBar {
            self.progress.clone().unwrap_or_default().size()
        } else {
            self.kind.size()
        }
    }
    pub fn input_key(&self, index: usize) -> String {
        self.name.clone().unwrap_or_else(|| format!("#{index}"))
    }
    pub fn bounds(&self) -> Bounds {
        Bounds {
            x: self.x,
            y: self.y,
            size: self.size(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub version: u32,
    pub canvas: Canvas,
    pub widgets: Vec<Placement>,
}

impl Default for Template {
    fn default() -> Self {
        Self {
            version: 1,
            canvas: Canvas {
                width: WIDTH,
                height: HEIGHT,
            },
            widgets: Vec::new(),
        }
    }
}

pub fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

impl Template {
    pub fn validate(&self) -> io::Result<()> {
        if self.version != 1 {
            return Err(invalid(format!(
                "Unsupported template version {} (expected 1)",
                self.version
            )));
        }
        if self.canvas.width != WIDTH || self.canvas.height != HEIGHT {
            return Err(invalid(format!("Template canvas must be {WIDTH}x{HEIGHT}")));
        }
        for (index, widget) in self.widgets.iter().enumerate() {
            if let Some(name) = &widget.name {
                if name.is_empty()
                    || name.len() > 64
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                {
                    return Err(invalid("Widget names must contain 1-64 ASCII letters, digits, underscores or hyphens"));
                }
                if self.widgets[..index]
                    .iter()
                    .any(|other| other.name.as_ref() == Some(name))
                {
                    return Err(invalid("Widget names must be unique"));
                }
            }
            if widget.kind == WidgetKind::ProgressBar {
                widget.progress.clone().unwrap_or_default().validate()?;
                if let Some(input) = &widget.input {
                    input.validate()?;
                }
                if widget.input == Some(InputSource::Api) && widget.name.is_none() {
                    return Err(invalid("API widgets require a name"));
                }
            } else if widget.progress.is_some() || widget.input.is_some() {
                return Err(invalid("Only Progress Bar currently accepts progress configuration and external inputs"));
            }
            if !widget.bounds().fits_canvas() {
                return Err(invalid(format!(
                    "Widget {} ({}) is outside the canvas",
                    index + 1,
                    widget.kind.name()
                )));
            }
            if widget.pattern.is_some() && widget.kind != WidgetKind::Life {
                return Err(invalid("Only Game of Life accepts a pattern"));
            }
            if self.widgets[..index]
                .iter()
                .any(|other| widget.bounds().intersects(other.bounds()))
            {
                return Err(invalid(format!(
                    "Widget {} ({}) overlaps another widget",
                    index + 1,
                    widget.kind.name()
                )));
            }
        }
        Ok(())
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)?.take(1_048_577).read_to_end(&mut bytes)?;
        if bytes.len() > 1_048_576 {
            return Err(invalid("Template exceeds 1 MiB"));
        }
        let template: Self =
            serde_json::from_slice(&bytes).map_err(|error| invalid(error.to_string()))?;
        template.validate()?;
        Ok(template)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, self)
            .map_err(|error| invalid(error.to_string()))?;
        writeln!(file)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    pub fn can_place(&self, bounds: Bounds, ignoring: Option<usize>) -> bool {
        bounds.fits_canvas()
            && self.widgets.iter().enumerate().all(|(index, widget)| {
                Some(index) == ignoring || !bounds.intersects(widget.bounds())
            })
    }

    pub fn first_slot(&self, kind: WidgetKind) -> Option<(usize, usize)> {
        self.first_slot_for(kind.size())
    }
    pub fn first_slot_for(&self, size: Size) -> Option<(usize, usize)> {
        for y in 0..=HEIGHT.checked_sub(size.height)? {
            for x in 0..=WIDTH.checked_sub(size.width)? {
                if self.can_place(Bounds { x, y, size }, None) {
                    return Some((x, y));
                }
            }
        }
        None
    }

    pub fn free_pixels(&self) -> usize {
        WIDTH * HEIGHT
            - self
                .widgets
                .iter()
                .map(|widget| {
                    let size = widget.size();
                    size.width * size.height
                })
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_templates_use_configured_sizes_and_validate_input_options() {
        let mut bar = Placement::new(WidgetKind::ProgressBar, 0, 0);
        bar.name = Some("download".into());
        bar.input = Some(InputSource::Api);
        bar.progress = Some(ProgressConfig {
            length: 9,
            width: 2,
            ..Default::default()
        });
        let mut template = Template {
            widgets: vec![bar.clone()],
            ..Default::default()
        };
        template.validate().unwrap();
        assert_eq!(template.free_pixels(), WIDTH * HEIGHT - 18);
        assert_eq!(template.first_slot(WidgetKind::ProgressBar), Some((0, 2)));
        assert_eq!(
            template.widgets[0].bounds().size,
            Size {
                width: 9,
                height: 2
            }
        );
        let encoded = serde_json::to_string(&template).unwrap();
        assert_eq!(
            serde_json::from_str::<Template>(&encoded).unwrap(),
            template
        );
        for change in 0..5 {
            let mut bad = bar.clone();
            match change {
                0 => bad.name = None,
                1 => bad.progress.as_mut().unwrap().length = 2,
                2 => {
                    bad.input = Some(InputSource::File {
                        path: "file".into(),
                        interval_ms: 0,
                        mask: Default::default(),
                    })
                }
                3 => bad.kind = WidgetKind::Clock,
                _ => bad.name = Some("invalid name".into()),
            }
            template.widgets[0] = bad;
            assert!(template.validate().is_err());
        }
        template.widgets = vec![bar.clone(), Placement { y: 3, ..bar }];
        assert!(template
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unique"));
        template.widgets[1].name = Some("second".into());
        template.widgets[1].y = 1;
        assert!(template
            .validate()
            .unwrap_err()
            .to_string()
            .contains("overlaps"));
    }

    fn placed(kind: WidgetKind, x: usize, y: usize) -> Placement {
        Placement {
            kind,
            x,
            y,
            ..Placement::new(kind, x, y)
        }
    }

    #[test]
    fn fitting_checks_contiguous_space_and_restores_after_removal() {
        let mut template = Template::default();
        template.widgets.push(placed(WidgetKind::Breathing, 0, 14));
        assert!(template.free_pixels() >= 7 * 11);
        assert_eq!(
            template.first_slot(WidgetKind::SevenSegmentClock),
            Some((0, 0))
        );
        assert_eq!(template.first_slot(WidgetKind::Clock), None);
        template
            .widgets
            .push(placed(WidgetKind::SystemMonitor, 0, 0));
        assert_eq!(template.first_slot(WidgetKind::Life), Some((0, 20)));
        assert_eq!(
            template.first_slot(WidgetKind::SevenSegmentClock),
            Some((0, 20))
        );
        template.widgets.push(placed(WidgetKind::Life, 0, 20));
        assert!(template.free_pixels() > 0);
        assert_eq!(template.first_slot(WidgetKind::SevenSegmentClock), None);
        template.widgets.remove(0);
        assert!(template.first_slot(WidgetKind::Breathing).is_some());
    }

    #[test]
    fn invalid_bounds_overlap_versions_and_options_are_rejected() {
        for widgets in [
            vec![placed(WidgetKind::SevenSegmentClock, usize::MAX, 0)],
            vec![placed(WidgetKind::SevenSegmentClock, 3, 0)],
            vec![placed(WidgetKind::Life, 0, 23)],
            vec![
                placed(WidgetKind::Life, 0, 0),
                placed(WidgetKind::Breathing, 0, 11),
            ],
            vec![Placement {
                pattern: Some(LifePattern::Glider),
                ..placed(WidgetKind::Clock, 0, 0)
            }],
        ] {
            let template = Template {
                widgets,
                ..Template::default()
            };
            assert!(template.validate().is_err());
        }
        let mut template = Template {
            version: 2,
            ..Template::default()
        };
        assert!(template.validate().is_err());
        template.version = 1;
        template.canvas.width = 10;
        assert!(template.validate().is_err());
    }

    #[test]
    fn template_roundtrip_and_replacement_preserve_layout_and_pattern() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("layout.json");
        let mut template = Template::default();
        template
            .widgets
            .push(placed(WidgetKind::SevenSegmentClock, 1, 0));
        template.widgets.push(Placement {
            kind: WidgetKind::Life,
            x: 0,
            y: 12,
            pattern: Some(LifePattern::Blinker),
            ..Placement::new(WidgetKind::Life, 0, 12)
        });
        template.save(&path).unwrap();
        assert_eq!(Template::load(&path).unwrap(), template);
        template.widgets.remove(0);
        template.save(&path).unwrap();
        assert_eq!(Template::load(&path).unwrap(), template);
        std::fs::write(&path, r#"{"version":1,"canvas":{"width":9,"height":34},"widgets":[{"kind":"unknown","x":0,"y":0}]}"#).unwrap();
        assert!(Template::load(&path).is_err());
        std::fs::write(&path, b"{broken").unwrap();
        assert!(Template::load(&path).is_err());
    }
}
