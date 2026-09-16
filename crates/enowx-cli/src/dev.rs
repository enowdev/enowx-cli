//! `enx dev`: watch the sources while the interface runs, then rebuild and
//! relaunch it on the same session so UI edits are visible immediately.
//!
//! This is a supervisor, not hot code swapping: a Rust binary cannot replace its
//! own machine code safely, so the closest honest equivalent is a fast rebuild
//! plus a session-preserving restart.

use std::{
    collections::HashMap,
    ffi::OsStr,
    io::{self, Write as _},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context as _, Result};
use enowx_core::{Config, SessionStore};

/// Sources a running interface is built from.
const WATCHED: [&str; 2] = ["crates", "Cargo.toml"];
const POLL: Duration = Duration::from_millis(250);
/// Let an editor finish writing before starting a build.
const SETTLE: Duration = Duration::from_millis(200);

pub fn run(workspace: Option<PathBuf>, session: Option<String>) -> Result<()> {
    let root = std::env::current_dir()?;
    anyhow::ensure!(
        root.join("Cargo.toml").exists() && root.join("crates").is_dir(),
        "Run `enx dev` from the Enx source checkout; no Cargo workspace here"
    );

    let store = SessionStore::default();
    // An explicit id pins one conversation; otherwise follow the newest session
    // in this workspace so a restart continues where the developer left off.
    let mut resume = match session {
        Some(id) => {
            store
                .load(&id)
                .with_context(|| format!("session {id} is not readable"))?;
            Some(id)
        }
        None => latest_session(&store)?,
    };

    let mut fingerprint = snapshot(&root)?;
    println!("enx dev: watching crates/ and Cargo.toml; edits reload the interface.");

    loop {
        if !build()? {
            println!("enx dev: build failed. Fix the error and save again.");
            fingerprint = wait_for_change(&root, fingerprint)?;
            continue;
        }

        let mut child = spawn(&root, &workspace, resume.as_deref())?;

        // Watch while the interface runs: the developer never leaves the UI.
        let changed = loop {
            if let Some(status) = child.try_wait().context("waiting for the interface")? {
                if !status.success() {
                    println!("enx dev: interface exited with {status}");
                }
                break false;
            }
            let current = snapshot(&root)?;
            if current != fingerprint {
                std::thread::sleep(SETTLE);
                fingerprint = snapshot(&root)?;
                break true;
            }
            std::thread::sleep(POLL);
        };

        if !changed {
            // The developer closed the interface: stop supervising.
            return Ok(());
        }

        stop(&mut child)?;
        // The killed child never ran its terminal restore, so undo its terminal
        // modes here before printing build output.
        restore_terminal();
        println!("enx dev: change detected, rebuilding…");
        resume = latest_session(&store)?.or(resume);
    }
}

fn spawn(root: &Path, workspace: &Option<PathBuf>, session: Option<&str>) -> Result<Child> {
    // Launch the built binary, not `cargo run`: the supervisor must be able to
    // stop the interface process itself rather than a cargo wrapper.
    let mut command = Command::new(root.join("target/debug/enx"));
    command.arg("tui");
    if let Some(workspace) = workspace {
        command.arg("--workspace").arg(workspace);
    }
    if let Some(session) = session {
        command.arg("--session").arg(session);
    }
    command
        .stdin(Stdio::inherit())
        .spawn()
        .context("starting the interface")
}

fn stop(child: &mut Child) -> Result<()> {
    child.kill().context("stopping the interface")?;
    child.wait().context("reaping the interface")?;
    Ok(())
}

/// Leave the alternate screen, restore the cursor, and disable mouse reporting.
fn restore_terminal() {
    let mut stdout = io::stdout();
    let _ = stdout.write_all(b"\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?2004l\x1b[?1049l\x1b[?25h");
    let _ = stdout.flush();
}

fn build() -> Result<bool> {
    print!("enx dev: building… ");
    io::stdout().flush().ok();
    let started = Instant::now();
    let status = Command::new(env!("CARGO"))
        .args(["build", "--quiet", "-p", "enx"])
        .status()
        .context("running cargo build")?;
    if status.success() {
        println!("ready in {:.1}s", started.elapsed().as_secs_f32());
    }
    Ok(status.success())
}

fn latest_session(store: &SessionStore) -> Result<Option<String>> {
    let workspace = std::fs::canonicalize(Config::load()?.workspace())?;
    for meta in store.list(50)? {
        // Only sessions recorded in this workspace can be resumed.
        if store
            .load(&meta.id)
            .is_ok_and(|session| session.workspace == workspace)
        {
            return Ok(Some(meta.id));
        }
    }
    Ok(None)
}

/// Modification times of every watched source file. Polling keeps this
/// dependency-free and behaves identically on every platform.
fn snapshot(root: &Path) -> Result<HashMap<PathBuf, SystemTime>> {
    let mut seen = HashMap::new();
    for entry in WATCHED {
        collect(&root.join(entry), &mut seen)?;
    }
    Ok(seen)
}

fn collect(path: &Path, seen: &mut HashMap<PathBuf, SystemTime>) -> Result<()> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    if meta.is_file() {
        if matches!(
            path.extension().and_then(OsStr::to_str),
            Some("rs" | "toml")
        ) {
            seen.insert(path.to_path_buf(), meta.modified()?);
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == OsStr::new("target") || name.to_string_lossy().starts_with('.') {
            continue;
        }
        collect(&entry.path(), seen)?;
    }
    Ok(())
}

fn wait_for_change(
    root: &Path,
    previous: HashMap<PathBuf, SystemTime>,
) -> Result<HashMap<PathBuf, SystemTime>> {
    loop {
        std::thread::sleep(POLL);
        if snapshot(root)? != previous {
            std::thread::sleep(SETTLE);
            return snapshot(root);
        }
    }
}
