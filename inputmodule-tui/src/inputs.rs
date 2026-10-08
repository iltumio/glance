//! Numeric widget inputs: one bounded latest-value slot per binding and one file worker.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const FILE_LIMIT: u64 = 1_048_576;
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn default_interval() -> u64 {
    1000
}
fn default_group() -> usize {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileMask {
    #[default]
    Number,
    JsonPointer {
        pointer: String,
    },
    Regex {
        pattern: String,
        #[serde(default = "default_group")]
        group: usize,
    },
}
impl FileMask {
    pub fn validate(&self) -> io::Result<()> {
        match self {
            Self::Number => Ok(()),
            Self::JsonPointer { pointer } => {
                if !pointer.is_empty() && !pointer.starts_with('/') {
                    return Err(invalid("JSON Pointer must be empty or start with /"));
                }
                for token in pointer.split('/') {
                    let mut chars = token.chars();
                    while let Some(c) = chars.next() {
                        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
                            return Err(invalid("Invalid JSON Pointer escape"));
                        }
                    }
                }
                Ok(())
            }
            Self::Regex { pattern, group } => {
                let regex = Regex::new(pattern).map_err(io::Error::other)?;
                if *group >= regex.captures_len() {
                    return Err(invalid("Regex capture group does not exist"));
                }
                Ok(())
            }
        }
    }
    pub fn extract(&self, text: &str) -> io::Result<f64> {
        let value = match self {
            Self::Number => text.trim().parse::<f64>().map_err(io::Error::other)?,
            Self::JsonPointer { pointer } => {
                let document: serde_json::Value =
                    serde_json::from_str(text).map_err(io::Error::other)?;
                let selected = document
                    .pointer(pointer)
                    .ok_or_else(|| invalid("JSON Pointer did not select a value"))?;
                selected
                    .as_f64()
                    .or_else(|| selected.as_str()?.parse().ok())
                    .ok_or_else(|| invalid("Selected JSON value is not numeric"))?
            }
            Self::Regex { pattern, group } => {
                // Compiled once by the worker in FileReader; also usable directly by tests.
                let regex = Regex::new(pattern).map_err(io::Error::other)?;
                capture(&regex, *group, text)?
            }
        };
        finite(value)
    }
}
fn finite(value: f64) -> io::Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid("Input value must be finite"))
    }
}
fn capture(regex: &Regex, group: usize, text: &str) -> io::Result<f64> {
    let captures = regex
        .captures(text)
        .ok_or_else(|| invalid("Regex did not match"))?;
    let value = captures
        .get(group)
        .ok_or_else(|| invalid("Regex capture is absent"))?
        .as_str()
        .parse()
        .map_err(io::Error::other)?;
    finite(value)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputSource {
    File {
        path: PathBuf,
        #[serde(default = "default_interval")]
        interval_ms: u64,
        #[serde(default)]
        mask: FileMask,
    },
    Api,
}
impl InputSource {
    pub fn validate(&self) -> io::Result<()> {
        if let Self::File {
            path,
            interval_ms,
            mask,
        } = self
        {
            if path.as_os_str().is_empty() {
                return Err(invalid("Input file path is empty"));
            }
            if !(100..=86_400_000).contains(interval_ms) {
                return Err(invalid("File interval_ms must be between 100 and 86400000"));
            }
            mask.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub name: String,
    pub source: Option<InputSource>,
    pub initial: f64,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub value: f64,
    pub error: Option<String>,
}
struct Slot {
    binding: Binding,
    snapshot: Snapshot,
    next_read: Instant,
    revision: u64,
}
#[derive(Default)]
struct State {
    slots: BTreeMap<String, Slot>,
    revision: u64,
    stopped: bool,
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}
#[derive(Clone, Default)]
pub struct InputHub(Arc<Shared>);
impl InputHub {
    pub fn snapshot(&self, name: &str) -> Option<Snapshot> {
        self.0
            .state
            .lock()
            .unwrap()
            .slots
            .get(name)
            .map(|s| s.snapshot.clone())
    }
    pub fn api_names(&self) -> Vec<String> {
        self.0
            .state
            .lock()
            .unwrap()
            .slots
            .iter()
            .filter(|(_, s)| s.binding.source == Some(InputSource::Api))
            .map(|(name, _)| name.clone())
            .collect()
    }
    pub fn api_update(&self, name: &str, value: f64) -> io::Result<Snapshot> {
        finite(value)?;
        let mut state = self.0.state.lock().unwrap();
        let slot = state
            .slots
            .get_mut(name)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Unknown widget"))?;
        if slot.binding.source != Some(InputSource::Api) {
            return Err(invalid("Widget does not accept API input"));
        }
        slot.snapshot = Snapshot { value, error: None };
        Ok(slot.snapshot.clone())
    }
    pub fn api_get(&self, name: &str) -> io::Result<Snapshot> {
        let state = self.0.state.lock().unwrap();
        let slot = state
            .slots
            .get(name)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Unknown widget"))?;
        if slot.binding.source != Some(InputSource::Api) {
            return Err(invalid("Widget does not accept API input"));
        }
        Ok(slot.snapshot.clone())
    }
}

pub struct InputService {
    hub: InputHub,
    worker: Option<JoinHandle<()>>,
    base: PathBuf,
}
impl InputService {
    pub fn new(base: &Path) -> Self {
        Self {
            hub: InputHub::default(),
            worker: None,
            base: base.to_owned(),
        }
    }
    pub fn hub(&self) -> InputHub {
        self.hub.clone()
    }
    pub fn reconcile(&mut self, bindings: Vec<Binding>) -> io::Result<()> {
        if self.worker.is_none()
            && bindings
                .iter()
                .any(|b| matches!(b.source, Some(InputSource::File { .. })))
        {
            let hub = self.hub.clone();
            let base = self.base.clone();
            self.worker = Some(
                thread::Builder::new()
                    .name("widget-file-input".into())
                    .spawn(move || file_worker(hub, base))?,
            );
        }
        let mut state = self.hub.0.state.lock().unwrap();
        let mut old = std::mem::take(&mut state.slots);
        for binding in bindings {
            let slot = match old.remove(&binding.name) {
                Some(slot) if slot.binding == binding => slot,
                _ => {
                    state.revision += 1;
                    Slot {
                        snapshot: Snapshot {
                            value: binding.initial,
                            error: None,
                        },
                        binding: binding.clone(),
                        next_read: Instant::now(),
                        revision: state.revision,
                    }
                }
            };
            state.slots.insert(binding.name.clone(), slot);
        }
        self.hub.0.wake.notify_all();
        Ok(())
    }
}
impl Drop for InputService {
    fn drop(&mut self) {
        self.hub.0.state.lock().unwrap().stopped = true;
        self.hub.0.wake.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct FileReader {
    mask: FileMask,
    regex: Option<Regex>,
}
impl FileReader {
    fn new(mask: FileMask) -> Self {
        let regex = if let FileMask::Regex { pattern, .. } = &mask {
            Regex::new(pattern).ok()
        } else {
            None
        };
        Self { mask, regex }
    }
    fn read(&self, path: &Path) -> io::Result<f64> {
        // Check before opening to avoid blocking on a FIFO/device path.
        let metadata = std::fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(invalid("Input must be a regular file"));
        }
        if metadata.len() > FILE_LIMIT {
            return Err(invalid("Input file exceeds 1 MiB"));
        }
        let file = File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err(invalid("Input must be a regular file"));
        }
        let mut text = String::new();
        file.take(FILE_LIMIT + 1).read_to_string(&mut text)?;
        if text.len() as u64 > FILE_LIMIT {
            return Err(invalid("Input file exceeds 1 MiB"));
        }
        if let (FileMask::Regex { group, .. }, Some(regex)) = (&self.mask, &self.regex) {
            capture(regex, *group, &text)
        } else {
            self.mask.extract(&text)
        }
    }
}
fn file_worker(hub: InputHub, base: PathBuf) {
    let mut readers: BTreeMap<String, (u64, FileReader)> = BTreeMap::new();
    loop {
        let mut state = hub.0.state.lock().unwrap();
        if state.stopped {
            return;
        }
        readers.retain(|name, (revision, _)| {
            state
                .slots
                .get(name)
                .is_some_and(|s| s.revision == *revision)
        });
        let now = Instant::now();
        let mut jobs = Vec::new();
        let mut wait = Duration::from_secs(30);
        for (name, slot) in &mut state.slots {
            if let Some(InputSource::File {
                path,
                interval_ms,
                mask,
            }) = &slot.binding.source
            {
                if slot.next_read <= now {
                    jobs.push((name.clone(), slot.revision, base.join(path), mask.clone()));
                    slot.next_read = now + Duration::from_millis(*interval_ms);
                }
                wait = wait.min(slot.next_read.saturating_duration_since(now));
            }
        }
        if jobs.is_empty() {
            let _ = hub.0.wake.wait_timeout(state, wait).unwrap();
            continue;
        }
        drop(state);
        for (name, revision, path, mask) in jobs {
            let (_, reader) = readers
                .entry(name.clone())
                .or_insert_with(|| (revision, FileReader::new(mask)));
            let result = reader.read(&path);
            let mut state = hub.0.state.lock().unwrap();
            if state.stopped {
                return;
            }
            if let Some(slot) = state
                .slots
                .get_mut(&name)
                .filter(|s| s.revision == revision)
            {
                match result {
                    Ok(value) => slot.snapshot = Snapshot { value, error: None },
                    Err(error) => slot.snapshot.error = Some(error.to_string()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_reader_rejects_nonfiles_oversized_and_non_utf8_content() {
        let directory = tempfile::tempdir().unwrap();
        let reader = FileReader::new(FileMask::Number);
        assert!(reader.read(directory.path()).is_err());
        let path = directory.path().join("input");
        File::create(&path)
            .unwrap()
            .set_len(FILE_LIMIT + 1)
            .unwrap();
        assert!(reader
            .read(&path)
            .unwrap_err()
            .to_string()
            .contains("1 MiB"));
        std::fs::write(&path, [255, 254]).unwrap();
        assert!(reader.read(&path).is_err());
    }

    #[test]
    fn masks_select_numbers_and_reject_missing_or_nonfinite_values() {
        let pointer = FileMask::JsonPointer {
            pointer: "/jobs/0/a~1b".into(),
        };
        assert_eq!(pointer.extract(r#"{"jobs":[{"a/b":42}]}"#).unwrap(), 42.0);
        assert!(pointer.extract("{}").is_err());
        let regex = FileMask::Regex {
            pattern: r"progress=(\d+(?:\.\d+)?)%".into(),
            group: 1,
        };
        assert_eq!(regex.extract("status: progress=12.5%").unwrap(), 12.5);
        assert!(regex.extract("no progress").is_err());
        assert!(FileMask::Number.extract("NaN").is_err());
        assert!(FileMask::JsonPointer {
            pointer: "/bad~2".into()
        }
        .validate()
        .is_err());
        assert!(FileMask::Regex {
            pattern: "(".into(),
            group: 1
        }
        .validate()
        .is_err());
        assert!(FileMask::Regex {
            pattern: "x".into(),
            group: 1
        }
        .validate()
        .is_err());
    }
    #[test]
    fn file_input_polls_keeps_last_good_value_recovers_and_stops_promptly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("value.json");
        std::fs::write(&path, "{\"p\":25}").unwrap();
        let mut service = InputService::new(dir.path());
        service
            .reconcile(vec![Binding {
                name: "file".into(),
                initial: 0.0,
                source: Some(InputSource::File {
                    path: "value.json".into(),
                    interval_ms: 100,
                    mask: FileMask::JsonPointer {
                        pointer: "/p".into(),
                    },
                }),
            }])
            .unwrap();
        let hub = service.hub();
        let await_state = |value, error| {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let s = hub.snapshot("file").unwrap();
                if s.value == value && s.error.is_some() == error {
                    break;
                }
                assert!(Instant::now() < deadline, "{s:?}");
                thread::sleep(Duration::from_millis(10));
            }
        };
        await_state(25.0, false);
        std::fs::write(&path, "broken").unwrap();
        await_state(25.0, true);
        std::fs::write(&path, "{\"p\":75}").unwrap();
        await_state(75.0, false);
        assert!(hub.api_update("file", 20.0).is_err());
        let started = Instant::now();
        drop(service);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
