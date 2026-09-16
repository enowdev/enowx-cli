# enowx-cli

A Rust coding agent with a terminal interface.

```
enowx                # open the terminal interface (default)
enowx config get model.default
enowx config set model.default anthropic/claude-sonnet-4.5
enowx config path
```

enowx-cli opens the TUI immediately; there is no onboarding screen. Open
`/provider`, choose a supported provider, then enter only its API key. Built-in
providers: **enxapi**, OpenAI, OpenRouter, Groq, and DeepSeek; Custom keeps
editable OpenAI-compatible endpoints. enxapi uses `https://enxapi.id/v1` and
`https://enxapi.id/v1/models`. Settings persist under `~/.enx/config.toml`.

## What is inside

| Piece | Notes |
|---|---|
| Agent loop, tools, sessions | Streaming model calls, paired tool results, JSONL sessions |
| Terminal interface | Framed layout, thought/tool cards, paged right sidebar, theme picker |
| Roles | Three shipped: Orchestrator, Writer, Researcher |

The default binary opens the terminal interface with five selectable palettes.
The web dashboard is available only with the optional `web` build feature.

## Roles

| Role | Tools | Purpose |
|---|---|---|
| Orchestrator | read, write, edit, glob, grep, bash, fetch, todo | Owns a task end to end, then verifies it |
| Writer | read, write, edit, glob, grep, todo | Documentation and prose. No shell |
| Researcher | read, glob, grep, fetch, todo | Read-only investigation |

Role filtering runs twice: unavailable tools are never advertised to the model,
and a call that arrives anyway is refused before dispatch.

## Terminal commands

`/help` `/new` `/sessions` `/resume <id>` `/role` `/model` `/provider`
`/reasoning` `/tools` `/theme` `/sidebar` `/tab 1..5` `/clear` `/stop`
`/status` `/skills` `/mcp` `/compact` `/quit`

Keys: `Enter` sends, `Ctrl+Enter` inserts a newline, `/` opens the palette,
`Ctrl+R` toggles reasoning, `Ctrl+O` toggles tool output, `PgUp`/`PgDn` scroll
the chat, and `Ctrl+C` clears the composer, opens the quit prompt when the
composer is empty, or interrupts a running turn.

`F1`–`F5` (or `Alt+1`–`Alt+5`) selects a telemetry tab without consuming typed
digits. `Ctrl+B` shows/hides the sidebar; on narrow terminals it opens over the
transcript while leaving the composer accessible.

Themes are changed only through `/theme`: arrow keys preview, `Enter` saves,
`Esc` cancels. Palettes: Obsidian Ice, Neo Acid, Chrome Void, OLED Stealth, and
Classic Amber. `NO_COLOR` is respected; unset it to see palette colours.

Mouse: drag over the transcript copies text to the clipboard on release, click
on a rendered file path opens it with the OS default application, and the
scroll wheel scrolls the transcript, popup selectors, or the composer field
depending on where the pointer is.

`/provider` opens the supported-provider list before showing any inputs.
Built-in base/model-list URLs are fixed; only Custom exposes endpoint fields.
`/model` is available after a provider is connected. Auto detect loads the
provider's full model list; selecting an entry activates and persists it
immediately.

## Configuration

`~/.enx/config.toml`, overridable by `ENX_API_KEY`, `ENX_BASE_URL`, `ENX_MODEL`,
`ENX_HOME`. `enowx config get provider.api_key` prints `(redacted)`; the key is
never echoed to the terminal.

| Key | Meaning |
|---|---|
| `model.default` | Model id sent to the provider |
| `model.context_window` | Window size used by the token gauge |
| `provider.base_url` | Any OpenAI-compatible endpoint |
| `provider.models_url` | Exact JSON endpoint used by Auto detect |
| `agent.max_steps` | Hard cap on model calls per turn |
| `agent.workspace` | Directory the file and shell tools are rooted in |
| `agent.shell_timeout_secs` | Kill a shell command after this long |
| `agent.auto_compact_at` | Fraction of the context window that triggers auto-compact |
| `agent.compact_keep_last` | Turns kept verbatim during compact |
| `ui.theme` | `obsidian_ice`, `neo_acid`, `chrome_void`, `oled_stealth`, or `classic` |
| `ui.currency` | Display currency for cost readouts (USD, IDR, JPY, …) |
| `ui.currency_rate` | Multiplier applied to USD prices for the display currency |

Sessions are stored as JSONL under `~/.enx/sessions`, one file per session,
including tool calls and results for replay. Interrupted calls without a
recorded result are marked unavailable rather than executed again
automatically. A session records the workspace it was created in and refuses to
resume against a different one.

Skills, MCP servers, and per-project agent instructions are discovered from
`.agents/`, `.enx/`, `.claude/`, `.cursor/`, `.gemini/`, and the standard
`~/.config` locations. `/skills` and `/mcp` open popups to toggle or add
entries; `/compact` folds older turns into a summary; auto-compact fires when
the context window nears its cap.

File tools reject paths and symlinks outside the workspace. `bash` runs with
the user's OS permissions and is **not a sandbox**; use only with trusted tasks
and providers.

## Build

```sh
cargo build --release                     # terminal only
cargo build --release --features web      # adds the optional `enowx serve` dashboard
cargo test --workspace
```

Install the binary under the `enowx` name:

```sh
which -a enowx                                     # expect no output before installing
install -m 755 target/release/enowx ~/.local/bin/enowx
```

## Live reload while developing

```sh
enowx dev                     # rebuild and relaunch the interface on every source change
enowx dev --session <id>      # pin one conversation across reloads
enowx tui --session <id>      # resume a session directly
```

Run `enowx dev` from this checkout. It watches `crates/` and `Cargo.toml`,
keeps the interface in the foreground, and on each save rebuilds and relaunches
it while resuming the newest session in this workspace.

For the optional dashboard, `bun run dev` in `web/` provides hot module
replacement and proxies `/api` to `enowx serve` on port 8787.

## Licence

MIT. See `LICENSE`.
