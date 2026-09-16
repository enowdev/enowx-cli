//! Image attachments for the composer: clipboard paste and drag-and-drop.
//!
//! Terminals cannot deliver binary clipboard data through stdin, so a paste of an
//! image is read from the system clipboard, and a drop arrives as a file path
//! that the terminal writes as text. Both paths end as a base64 `data:` URL, so a
//! session replays without needing the original file.
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use enowx_core::message::Attachment;

/// Extensions the OpenAI-compatible image part accepts.
const IMAGE_TYPES: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
];
/// Providers reject oversized inline images; refuse early with a clear message
/// instead of sending a request that will fail.
const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Text form of an inline attachment chip: `[Image 3]`. Backspace treats the
/// whole `[Image N]` run as one editable token so the user removes the image by
/// deleting the chip, no separate command needed.
pub fn chip(index: usize) -> String {
    format!("[Image {}]", index + 1)
}

/// Extract the chip index the caret is on, or the one immediately before it.
/// Returns the byte range so callers can delete the chip as a single token.
pub fn chip_at(text: &str, cursor: usize) -> Option<(std::ops::Range<usize>, usize)> {
    let start = text[..cursor].rfind('[')?;
    let inside = &text[start..];
    let end = start + inside.find(']').map(|i| i + 1)?;
    if end < cursor {
        return None;
    }
    let inner = text.get(start + 1..end - 1)?.strip_prefix("Image ")?;
    inner
        .parse::<usize>()
        .ok()
        .filter(|n| *n >= 1)
        .map(|n| (start..end, n - 1))
}

/// Strip every `[Image N]` chip so the model never sees the placeholder token.
/// Contiguous whitespace is collapsed to keep the surrounding prose readable.
pub fn strip_chips(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[Image ") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find(']') {
            let inner = &rest[start + 7..start + end];
            if inner.chars().all(|c| c.is_ascii_digit()) && !inner.is_empty() {
                rest = &rest[start + end + 1..];
                continue;
            }
        }
        out.push_str(&rest[start..start + 1]);
        rest = &rest[start + 1..];
    }
    out.push_str(rest);
    // Collapse the double spaces the chip leaves behind, keep newlines intact.
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for ch in out.chars() {
        if ch == ' ' {
            if prev_space {
                continue;
            }
            prev_space = true;
        } else {
            prev_space = false;
        }
        collapsed.push(ch);
    }
    collapsed.trim().to_owned()
}

pub fn mime_for(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    IMAGE_TYPES
        .iter()
        .find(|(suffix, _)| *suffix == extension)
        .map(|(_, mime)| *mime)
}

/// Read an image file into an attachment.
pub fn from_path(path: &Path) -> Result<Attachment> {
    let mime =
        mime_for(path).with_context(|| format!("{} is not a supported image", path.display()))?;
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    anyhow::ensure!(
        bytes.len() <= MAX_BYTES,
        "{} is {:.1} MB; the limit is 8 MB",
        path.display(),
        bytes.len() as f64 / 1_048_576.0
    );
    Ok(Attachment {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".into()),
        data_url: encode(mime, &bytes),
    })
}

/// Interpret typed or dropped text as file paths. A terminal reports a drop as
/// the dropped paths, quoted or escaped depending on the terminal.
pub fn dropped_paths(text: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for candidate in split_paths(text) {
        let path = PathBuf::from(&candidate);
        if mime_for(&path).is_some() && path.is_file() {
            paths.push(path);
        }
    }
    paths
}

/// Split on unescaped whitespace, honouring quotes and backslash escapes so a
/// path containing spaces survives the round trip.
fn split_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = text.trim().chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '\'' | '"' => match quote {
                Some(open) if open == character => quote = None,
                Some(_) => current.push(character),
                None => quote = Some(character),
            },
            character if character.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            character => current.push(character),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Read an image from the system clipboard, if it holds one.
#[cfg(target_os = "macos")]
pub fn from_clipboard() -> Result<Attachment> {
    // Try `pbpaste` first: it is available on every macOS install, needs no
    // Automation permission dialog, and writes the decoded image straight to
    // stdout. AppleScript is the fallback because some sandboxed terminals
    // block `osascript` altogether.
    for (mime, args) in [
        ("image/png", vec!["-Prefer", "png"]),
        ("image/tiff", vec!["-Prefer", "tiff"]),
    ] {
        let Ok(output) = std::process::Command::new("pbpaste").args(&args).output() else {
            continue;
        };
        // pbpaste returns 0 and writes UTF-8 text when the clipboard is not an
        // image, so we treat a plausible image header as the signal instead of
        // trusting the exit code.
        if !output.status.success() || output.stdout.len() < 16 {
            continue;
        }
        if !looks_like_image(&output.stdout) {
            continue;
        }
        anyhow::ensure!(
            output.stdout.len() <= MAX_BYTES,
            "the clipboard image is {:.1} MB; the limit is 8 MB",
            output.stdout.len() as f64 / 1_048_576.0
        );
        return Ok(Attachment {
            name: "clipboard.png".into(),
            data_url: encode(mime, &output.stdout),
        });
    }

    // AppleScript fallback: writes the PNG data to a temp file, then reads it.
    let target = std::env::temp_dir().join(format!("enx-paste-{}.png", uuid()));
    let script = format!(
        "set target to POSIX file \"{}\"\n\
         try\n\
             set image to the clipboard as «class PNGf»\n\
         on error\n\
             return \"no-image\"\n\
         end try\n\
         set handle to open for access target with write permission\n\
         set eof handle to 0\n\
         write image to handle\n\
         close access handle\n\
         return \"ok\"",
        target.display()
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .context("reading the clipboard with osascript")?;
    let verdict = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if verdict != "ok" {
        let _ = std::fs::remove_file(&target);
        anyhow::bail!("the clipboard does not contain an image");
    }
    let attachment = from_path(&target);
    let _ = std::fs::remove_file(&target);
    attachment
}

#[cfg(target_os = "macos")]
fn looks_like_image(bytes: &[u8]) -> bool {
    // PNG, JPEG, GIF, TIFF (both endians), WebP, BMP.
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF8")
        || bytes.starts_with(b"II*\0")
        || bytes.starts_with(b"MM\0*")
        || (bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"BM")
}

#[cfg(not(target_os = "macos"))]
pub fn from_clipboard() -> Result<Attachment> {
    // Wayland and X11 helpers write the image to stdout, so no temporary file is
    // needed; the first tool that reports an image type wins.
    for (program, args) in [
        ("wl-paste", vec!["--type", "image/png"]),
        (
            "xclip",
            vec!["-selection", "clipboard", "-t", "image/png", "-o"],
        ),
    ] {
        let Ok(output) = std::process::Command::new(program).args(&args).output() else {
            continue;
        };
        if output.status.success() && !output.stdout.is_empty() {
            anyhow::ensure!(
                output.stdout.len() <= MAX_BYTES,
                "the clipboard image is {:.1} MB; the limit is 8 MB",
                output.stdout.len() as f64 / 1_048_576.0
            );
            return Ok(Attachment {
                name: "clipboard.png".into(),
                data_url: encode("image/png", &output.stdout),
            });
        }
    }
    anyhow::bail!("no clipboard image; install wl-clipboard or xclip")
}

/// Copy plain text to the OS clipboard. Best effort: silently drops on
/// unsupported platforms so a failed paste never crashes the TUI.
pub fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    #[cfg(target_os = "macos")]
    let candidates: &[(&str, &[&str])] = &[("pbcopy", &[])];
    #[cfg(target_os = "linux")]
    let candidates: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];
    #[cfg(target_os = "windows")]
    let candidates: &[(&str, &[&str])] = &[("clip", &[])];
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let candidates: &[(&str, &[&str])] = &[];

    for (bin, args) in candidates {
        let mut cmd = std::process::Command::new(bin);
        cmd.args(*args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let Ok(mut child) = cmd.spawn() else { continue };
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(text.as_bytes()).is_err() {
                continue;
            }
        }
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }
    false
}

/// Open a file or directory with the OS default application. Best effort:
/// unsupported platforms or a failed spawn silently return `false` so a
/// click in the transcript never crashes the TUI.
pub fn open_path(path: &std::path::Path) -> bool {
    #[cfg(target_os = "macos")]
    let (bin, args): (&str, &[&str]) = ("open", &[]);
    #[cfg(target_os = "linux")]
    let (bin, args): (&str, &[&str]) = ("xdg-open", &[]);
    #[cfg(target_os = "windows")]
    let (bin, args): (&str, &[&str]) = ("cmd", &["/C", "start", ""]);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let (bin, args): (&str, &[&str]) = ("", &[]);
    if bin.is_empty() {
        return false;
    }
    let mut cmd = std::process::Command::new(bin);
    cmd.args(args)
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.spawn().is_ok()
}

#[cfg(target_os = "macos")]
fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}")
}

fn encode(mime: &str, bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4 + mime.len() + 20);
    out.push_str("data:");
    out.push_str(mime);
    out.push_str(";base64,");
    for chunk in bytes.chunks(3) {
        let block = ((chunk[0] as u32) << 16)
            | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
            | (*chunk.get(2).unwrap_or(&0) as u32);
        out.push(ALPHABET[(block >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(block >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(block >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(block & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{dropped_paths, encode, mime_for, split_paths};
    use std::path::Path;

    #[test]
    fn only_supported_image_extensions_are_recognised() {
        assert_eq!(mime_for(Path::new("a/b.PNG")), Some("image/png"));
        assert_eq!(mime_for(Path::new("shot.jpeg")), Some("image/jpeg"));
        assert_eq!(mime_for(Path::new("notes.txt")), None);
        assert_eq!(mime_for(Path::new("noextension")), None);
    }

    #[test]
    fn dropped_text_splits_quoted_and_escaped_paths() {
        assert_eq!(
            split_paths(r"'/tmp/one two.png' /tmp/three.png"),
            ["/tmp/one two.png", "/tmp/three.png"]
        );
        assert_eq!(split_paths(r"/tmp/a\ b.png"), ["/tmp/a b.png"]);
    }

    #[test]
    fn dropped_paths_ignores_text_and_missing_files() {
        // Nothing here exists on disk, so nothing is accepted as an attachment.
        assert!(dropped_paths("just a sentence").is_empty());
        assert!(dropped_paths("/tmp/definitely-missing-image.png").is_empty());
    }

    #[test]
    fn base64_padding_matches_input_length() {
        assert_eq!(encode("image/png", b"a"), "data:image/png;base64,YQ==");
        assert_eq!(encode("image/png", b"ab"), "data:image/png;base64,YWI=");
        assert_eq!(encode("image/png", b"abc"), "data:image/png;base64,YWJj");
    }
}
