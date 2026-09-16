use super::*;
use crate::attachments::{chip, from_clipboard, from_path};
use std::path::{Path, PathBuf};

impl App {
    /// Populate the attach picker with images found in the workspace.
    pub(crate) fn open_attach(&mut self) -> anyhow::Result<()> {
        use crate::attachments::mime_for;
        use ignore::WalkBuilder;
        let workspace = self.config.workspace();
        let mut items: Vec<(String, String)> = Vec::new();
        for entry in WalkBuilder::new(&workspace)
            .hidden(false)
            // Ignore filters here would hide screenshots or downloads that a
            // typical `.gitignore` blocks; the picker deliberately shows every
            // real image in the workspace, subject to a hard 200-item cap.
            .git_ignore(false)
            .git_exclude(false)
            .git_global(false)
            .ignore(false)
            .parents(false)
            .build()
            .flatten()
        {
            if entry.file_type().is_some_and(|kind| kind.is_file())
                && mime_for(entry.path()).is_some()
            {
                let rel = entry
                    .path()
                    .strip_prefix(&workspace)
                    .unwrap_or(entry.path())
                    .display()
                    .to_string();
                let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                items.push((
                    entry.path().display().to_string(),
                    format!("{rel} · {:.1} KB", size as f64 / 1024.0),
                ));
                if items.len() >= 200 {
                    break;
                }
            }
        }
        if items.is_empty() {
            self.push(
                crate::session::TranscriptKind::System,
                "No images found in this workspace. Use /attach <path> for a specific file.",
            );
            return Ok(());
        }
        items.sort_by(|a, b| a.1.cmp(&b.1));
        self.modal_items = items;
        self.modal = crate::modal::Modal::Attach;
        self.modal_cursor = 0;
        Ok(())
    }

    pub(crate) fn attach_from_clipboard(&mut self) {
        match from_clipboard() {
            Ok(image) => self.push_attachment(image),
            Err(error) => self.attach_error = Some(format!("{error:#}")),
        }
    }

    pub(crate) fn attach_paths(&mut self, paths: &[PathBuf]) {
        let mut error: Option<String> = None;
        for path in paths {
            match from_path(path) {
                Ok(image) => self.push_attachment(image),
                Err(cause) if error.is_none() => error = Some(format!("{cause:#}")),
                Err(_) => {}
            }
        }
        self.attach_error = error;
    }

    pub(crate) fn attach_from_path(&mut self, path: &Path) {
        self.attach_paths(&[path.to_path_buf()]);
    }

    fn push_attachment(&mut self, image: enowx_core::message::Attachment) {
        let mark = chip(self.attachments.len());
        self.attachments.push(image);
        self.attach_error = None;
        // Insert as a single chip token; a leading space keeps the chip from
        // fusing with the character before it when the caret sat mid-word.
        let needs_space =
            self.cursor > 0 && !self.input[..self.cursor].ends_with(|c: char| c.is_whitespace());
        let insert = if needs_space {
            format!(" {mark}")
        } else {
            mark
        };
        self.input.insert_str(self.cursor, &insert);
        self.cursor += insert.len();
    }

    /// Rebuild chip numbering after a deletion so `[Image 1] [Image 2]` stays
    /// contiguous even when the user removes the first one.
    pub(crate) fn renumber_chips(&mut self) {
        let mut out = String::with_capacity(self.input.len());
        let mut rest = self.input.as_str();
        let mut next = 0usize;
        let mut cursor_shift = 0isize;
        let mut consumed = 0usize;
        while let Some(start) = rest.find("[Image ") {
            out.push_str(&rest[..start]);
            let tail = &rest[start..];
            if let Some(end) = tail.find(']') {
                let inner = &tail[7..end];
                if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                    let replacement = chip(next);
                    let original_end = consumed + start + end + 1;
                    if self.cursor >= original_end {
                        cursor_shift += replacement.len() as isize - (end + 1) as isize;
                    }
                    out.push_str(&replacement);
                    next += 1;
                    consumed += start + end + 1;
                    rest = &tail[end + 1..];
                    continue;
                }
            }
            out.push_str(&tail[..1]);
            consumed += start + 1;
            rest = &tail[1..];
        }
        out.push_str(rest);
        self.input = out;
        let new_cursor = self.cursor as isize + cursor_shift;
        self.cursor = new_cursor.clamp(0, self.input.len() as isize) as usize;
    }
}
