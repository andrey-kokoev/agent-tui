use crate::control_jsonl::{ControlJsonlEntry, ControlJsonlError, parse_control_jsonl};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct ControlJsonlWatcherState {
    pub byte_offset: u64,
    pub next_line_number: usize,
    partial_line: String,
}

#[derive(Debug)]
pub struct ControlJsonlPollResult {
    pub entries: Vec<ControlJsonlEntry>,
    pub errors: Vec<ControlJsonlError>,
    pub bytes_read: usize,
}

#[derive(Debug)]
pub struct ControlJsonlWatcher {
    path: PathBuf,
    state: ControlJsonlWatcherState,
}

impl ControlJsonlWatcher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            state: ControlJsonlWatcherState {
                byte_offset: 0,
                next_line_number: 1,
                partial_line: String::new(),
            },
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn state(&self) -> &ControlJsonlWatcherState {
        &self.state
    }

    pub fn poll_once(&mut self) -> Result<ControlJsonlPollResult, String> {
        poll_control_jsonl_path(&self.path, &mut self.state)
    }
}

pub fn poll_control_jsonl_path(
    path: &Path,
    state: &mut ControlJsonlWatcherState,
) -> Result<ControlJsonlPollResult, String> {
    let mut file =
        File::open(path).map_err(|error| format!("control_jsonl_open_failed:{error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("control_jsonl_metadata_failed:{error}"))?;

    if metadata.len() < state.byte_offset {
        state.byte_offset = 0;
        state.next_line_number = 1;
        state.partial_line.clear();
    }

    file.seek(SeekFrom::Start(state.byte_offset))
        .map_err(|error| format!("control_jsonl_seek_failed:{error}"))?;

    let mut appended = String::new();
    let bytes_read = file
        .read_to_string(&mut appended)
        .map_err(|error| format!("control_jsonl_read_failed:{error}"))?;
    state.byte_offset += bytes_read as u64;

    let mut content = String::new();
    content.push_str(&state.partial_line);
    content.push_str(&appended);

    if content.is_empty() {
        return Ok(ControlJsonlPollResult {
            entries: Vec::new(),
            errors: Vec::new(),
            bytes_read,
        });
    }

    let complete_content = if content.ends_with('\n') || content.ends_with('\r') {
        state.partial_line.clear();
        content
    } else if let Some((complete, partial)) = content.rsplit_once('\n') {
        state.partial_line = partial.to_string();
        format!("{complete}\n")
    } else {
        state.partial_line = content;
        return Ok(ControlJsonlPollResult {
            entries: Vec::new(),
            errors: Vec::new(),
            bytes_read,
        });
    };

    let base_line_number = state.next_line_number;
    let consumed_line_count = complete_content.lines().count();
    state.next_line_number += consumed_line_count;

    let (mut entries, mut errors) = parse_control_jsonl(&complete_content);
    for entry in &mut entries {
        entry.line_number += base_line_number - 1;
    }
    for error in &mut errors {
        error.line_number += base_line_number - 1;
    }

    Ok(ControlJsonlPollResult {
        entries,
        errors,
        bytes_read,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{OpenOptions, remove_file};
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    const CONTROL_FIXTURE: &str =
        include_str!("../../narada/packages/carrier-protocol/fixtures/control-input-event.json");

    fn temp_control_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock works")
            .as_nanos();
        std::env::temp_dir().join(format!("narada-agent-tui-control-{unique}.jsonl"))
    }

    fn append(path: &Path, content: &str) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open temp control file");
        file.write_all(content.as_bytes())
            .expect("append temp control file");
    }

    #[test]
    fn polls_only_appended_complete_lines() {
        let path = temp_control_path();
        append(&path, "\n");
        append(&path, CONTROL_FIXTURE.trim_end());
        append(&path, "\n");

        let mut watcher = ControlJsonlWatcher::new(&path);
        let first = watcher.poll_once().expect("first poll succeeds");
        assert!(first.errors.is_empty());
        assert_eq!(first.entries.len(), 1);
        assert_eq!(first.entries[0].line_number, 2);

        append(&path, CONTROL_FIXTURE.trim_end());
        append(&path, "\n");
        let second = watcher.poll_once().expect("second poll succeeds");
        assert!(second.errors.is_empty());
        assert_eq!(second.entries.len(), 1);
        assert_eq!(second.entries[0].line_number, 3);

        remove_file(path).ok();
    }

    #[test]
    fn holds_partial_line_until_newline_arrives() {
        let path = temp_control_path();
        let fixture = CONTROL_FIXTURE.trim_end();
        let split = fixture.split_at(fixture.len() / 2);
        append(&path, split.0);

        let mut watcher = ControlJsonlWatcher::new(&path);
        let first = watcher.poll_once().expect("first poll succeeds");
        assert!(first.entries.is_empty());
        assert!(first.errors.is_empty());

        append(&path, split.1);
        append(&path, "\n");
        let second = watcher.poll_once().expect("second poll succeeds");
        assert_eq!(second.entries.len(), 1);
        assert!(second.errors.is_empty());

        remove_file(path).ok();
    }

    #[test]
    fn resets_after_truncation() {
        let path = temp_control_path();
        append(&path, CONTROL_FIXTURE.trim_end());
        append(&path, "\n\n");

        let mut watcher = ControlJsonlWatcher::new(&path);
        assert_eq!(watcher.poll_once().expect("first poll").entries.len(), 1);

        std::fs::write(&path, format!("{}\n", CONTROL_FIXTURE.trim_end()))
            .expect("truncate temp file");
        let second = watcher.poll_once().expect("second poll after truncate");
        assert_eq!(second.entries.len(), 1);
        assert_eq!(second.entries[0].line_number, 1);

        remove_file(path).ok();
    }
}
