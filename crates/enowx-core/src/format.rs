//! Post-write formatter: after `WriteTool` stages a file, look up the
//! project's usual formatter for that language and run it. Skipped silently
//! when the formatter is not installed; the TUI can then prompt the user to
//! install it (`format::MissingFormatter`).

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// The formatter chosen for one language, plus the install command the UI
/// can suggest when it is missing.
pub struct Formatter {
    pub language: &'static str,
    pub bin: &'static str,
    /// Args passed with `bin` when the file argument is appended.
    pub args: &'static [&'static str],
    /// Human-readable install hint shown in the missing-formatter popup.
    pub install_hint: &'static str,
    /// The exact command the popup can run when the user consents.
    pub install_cmd: &'static [&'static str],
}

/// Pick the formatter for a file based on its extension. Returns `None` for
/// languages we do not format (Markdown, plaintext, unknown).
pub fn formatter_for(path: &Path) -> Option<Formatter> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    Some(match ext {
        "rs" => Formatter {
            language: "rust",
            bin: "rustfmt",
            args: &["--edition", "2021", "--emit", "files"],
            install_hint: "install rustfmt via rustup",
            install_cmd: &["rustup", "component", "add", "rustfmt"],
        },
        "ts" | "tsx" | "js" | "jsx" | "json" | "css" | "scss" | "html" | "yaml" | "yml" => {
            Formatter {
                language: "typescript",
                bin: "prettier",
                args: &["--write"],
                install_hint: "install prettier via npm",
                install_cmd: &["npm", "install", "-g", "prettier"],
            }
        }
        "py" => Formatter {
            language: "python",
            bin: "ruff",
            args: &["format"],
            install_hint: "install ruff via pip",
            install_cmd: &["pip", "install", "--user", "ruff"],
        },
        "go" => Formatter {
            language: "go",
            bin: "gofmt",
            args: &["-w"],
            install_hint: "install Go toolchain",
            install_cmd: &["go", "install", "golang.org/x/tools/cmd/goimports@latest"],
        },
        _ => return None,
    })
}

/// True when the formatter's binary is on PATH. Cheap check that avoids
/// spawning the process when we know it will fail.
pub fn is_available(bin: &str) -> bool {
    which::which(bin).is_ok()
}

/// Run the formatter over `path`. Returns `Ok(true)` on success, `Ok(false)`
/// when the formatter is not installed (caller can prompt for install), and
/// `Err` for actual formatter errors.
pub async fn format_file(fmt: &Formatter, path: &Path) -> Result<bool, String> {
    if !is_available(fmt.bin) {
        return Ok(false);
    }
    let mut cmd = Command::new(fmt.bin);
    cmd.args(fmt.args)
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    match cmd.output().await {
        Ok(out) if out.status.success() => Ok(true),
        Ok(out) => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
        Err(e) => Err(e.to_string()),
    }
}
