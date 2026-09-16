use super::{string_arg, truncate_output, Tool, ToolCtx, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;

pub(super) struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Run one shell command in the workspace. stdout and stderr are returned together."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let command = string_arg(&args, "command")?;
        let mut spawner = Command::new("/bin/sh");
        spawner
            .arg("-lc")
            .arg(command)
            .current_dir(&ctx.workspace)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        // Its own process group, so a stop or timeout reaches the whole pipeline
        // rather than leaving `sh`'s children running.
        #[cfg(unix)]
        spawner.process_group(0);
        let mut child = spawner.spawn().context("starting shell")?;
        let group = child.id().map(|pid| pid as i32);
        let kill_group = move || {
            #[cfg(unix)]
            if let Some(pid) = group {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        };

        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let wait =
            async { tokio::try_join!(child.wait(), capture_pipe(stdout), capture_pipe(stderr)) };
        let (status, stdout, stderr) = tokio::select! {
            result = tokio::time::timeout(ctx.shell_timeout, wait) => match result {
                Ok(output) => output.context("collecting command output")?,
                Err(_) => {
                    kill_group();
                    let _ = child.wait().await;
                    anyhow::bail!("command timed out after {} seconds", ctx.shell_timeout.as_secs());
                }
            },
            _ = ctx.cancel.cancelled() => {
                kill_group();
                let _ = child.wait().await;
                anyhow::bail!("command cancelled");
            }
        };

        let mut text = String::from_utf8_lossy(&stdout).into_owned();
        let stderr = String::from_utf8_lossy(&stderr);
        if !stderr.is_empty() {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&stderr);
        }
        let exit = status
            .code()
            .map_or_else(|| "signal".to_string(), |code| code.to_string());
        truncate_output(&mut text, 80_000);
        let content = format!("exit {exit}\n{}", text.trim_end());
        Ok(if status.success() {
            ToolOutput::ok(content)
        } else {
            ToolOutput::error(content)
        })
    }
}

async fn capture_pipe(mut pipe: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = pipe.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let retained = count.min(80_000usize.saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..retained]);
        truncated |= retained < count;
    }
    if truncated {
        output.extend_from_slice(b"\n[output truncated]");
    }
    Ok(output)
}
