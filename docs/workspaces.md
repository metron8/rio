# Project workspaces

A workspace is a named, ordinary Rio tab with split terminal panels. Opening an already-open workspace focuses its tab, keeping its processes, pane selection and terminal contents. The identity belongs to the tab, so tab reordering and closing do not leave stale references.

## Build and run

Requires Rust 1.96.1 (rustup reads the repository's rust-toolchain.toml). On macOS install Xcode or its command line tools and accept its license in your terminal. Native Metal builds need no additional Homebrew libraries.

```sh
cargo build -p rioterm --locked
cargo run -p rioterm --locked
```

Merge [the example](../extra/workspaces/config.toml) into `~/.config/rio/config.toml`, changing `root` to real project directories. Merge existing `[bindings]` / `[navigation]` tables rather than repeating them. Use `[navigation] mode = "Tab"` (the default). `Plain` also uses internal tabs; `NativeTab` is rejected with an explanatory warning because it represents tabs as separate native windows.

For an isolated trial without changing your normal Rio settings:

```sh
mkdir -p /tmp/rio-workspace-demo
cp extra/workspaces/config.toml /tmp/rio-workspace-demo/config.toml
# Edit roots in /tmp/rio-workspace-demo/config.toml first.
RIO_CONFIG_HOME=/tmp/rio-workspace-demo cargo run -p rioterm --locked
```

Open the standard command palette (Cmd-Shift-P on macOS) and choose **Open Workspace**, or assign `OpenWorkspacePalette` directly using Rio's normal binding format:

```toml
[bindings]
keys = [
  { key = "w", with = "super | shift", action = "OpenWorkspacePalette" },
  # Modifier-free alternative:
  # { key = "F6", action = "OpenWorkspacePalette" },
]
```

Type a name to fuzzy-search; use Up/Down/Tab to select, Enter to open/focus, Escape to dismiss. **Open** marks workspaces already open in this window. Mouse selection and clipboard paste are supported too. The system macOS Paste menu follows the same overlay input routing as physical keys, so it cannot write into a terminal behind the palette.

## Configuration

```toml
[[workspaces]]
id = "backend"                 # Stable identity; ASCII letters/digits/-/_
name = "Backend"               # Display name, searchable in the palette
root = "~/projects/backend"    # Absolute path or ~/ (no variable expansion)

[[workspaces.panels]]          # User's default shell at root

[[workspaces.panels]]
working-dir = "api"            # Relative to root, must not contain ..
command = { program = "cargo", args = ["watch", "-x", "test"] }

[[workspaces.panels]]
split-down = true              # Split the preceding successful panel below
# Without split-down, later panels split the preceding panel to the right.
```

At least one panel is required. Panels are created in declaration order; the first successful panel fills the tab, later successful panels use Rio's existing split layout. The last successfully created panel is initially focused. A panel without `command` starts the user's default shell (the normal Rio default-shell spawn path), even when the initial window was launched with `rio -e ...`. It does not inherit a configured top-level custom program. Shell integration follows the existing preference for default shells; explicit commands are launched as given, without injected shell arguments.

Commands are passed as a program and an argument vector. They are never joined into shell text. For shell syntax use an explicit shell, for example `command = { program = "/bin/sh", args = ["-c", "printf 'hello\\n'; exec /bin/sh"] }`.

Config parsing checks duplicate/invalid IDs, blank names/programs, empty panel sets, malformed command args, NULs, nonrelative panel paths and unknown workspace/panel/command keys. Root/panel directory existence and launch errors are checked on explicit opening. A missing mount does not invalidate otherwise correct configuration. Workspace PTYs use Rio's existing spawn implementation even if `use-fork = true`, since forkpty currently cannot apply per-panel directories or synchronously report exec failures.

## Lifecycle and limits

- Uniqueness is **per window**. Two windows may intentionally open separate instances of the same recipe. This was the minimal architectural choice; global cross-window switching remains a possible later enhancement.
- No workspace command runs on Rio startup or config reload. Only explicit opening launches commands.
- Reload updates recipes for future openings. Live tabs are not rebuilt, renamed or closed. A running tab whose recipe was removed remains in the palette until closed.
- If some panels fail, successfully launched panels stay open and a warning reports the failed panel numbers, directories or commands. Repeat selection focuses this partial tab and does not retry failed panels. Close/reopen to retry the recipe. If all fail, no workspace tab is registered and opening can be retried immediately.
- Once a successfully launched command exits, Rio's normal child-exit behavior removes its panel. A program that starts successfully and then exits with an error is treated as a normal terminal program, not a synchronous spawn failure.
- Close a single panel with `CloseSplitOrTab`; close the workspace with standard `CloseTab`, its tab close button, or the command palette's Close Tab. These invoke unchanged Rio context teardown and window/quit confirmation paths. Closing one panel does not restart it on subsequent workspace selection.
- Descriptions persist in config.toml; processes and live layouts do not survive exiting Rio. No daemon, detach/reattach, saved process memory, automatic restore, background network service or VT/GPU changes.
- Newly created ordinary splits/tabs retain Rio's normal shell/CWD behavior; they do not alter the saved workspace recipe.

## Checks performed on macOS arm64

Base SHA and source map: [investigation](workspaces-research.md). The previous sheepshead checkout and normal Rio configuration were left untouched.

| Check | Command / result |
| --- | --- |
| Clean unmodified base build | `cargo build -p rioterm --locked`, new target directory, passed (1m02s). |
| Base UI launch | Same binary in temporary app bundle, isolated RIO_CONFIG_HOME; typed `printf "RIO_BASE_OK\n"` and visually verified output/prompt. |
| Clean modified build | `CARGO_TARGET_DIR=/tmp/rio-workspace-clean-target cargo build -p rioterm --locked`, new target directory, passed (59.48s). |
| Formatting | `cargo fmt --all -- --check`, passed. |
| All-workspace native CI lint | `cargo clippy --all-targets --all-features --locked`, passed with one pre-existing `clippy::question_mark` warning in unchanged `rio-vt/src/performer/mod.rs:474`. |
| Strict lint of changed packages | `cargo clippy -p rio-backend -p rioterm --all-targets --all-features --no-deps --locked -- -D warnings`, passed. |
| Full tests including docs | `cargo test --features wgpu --locked`, 1421 passed, 0 failed, 11 upstream tests ignored, across 39 test-result groups. |
| Focused workspace tests | `cargo test -p rio-backend -p rioterm workspace --locked`: config validation/roundtrip/example, palette matching/navigation/open marker, standard binding parse, repeat/reorder identity, missing directories/capacity/native mode, real PTY PID survival, partial/all spawn errors, exact argv and root-relative CWD (including spaces). Included in the full run above. |

`make lint`'s stricter whole-workspace `-D warnings` command fails on that existing performer warning. `git diff BASE -- rio-vt/src/performer/mod.rs` is empty; it was not changed because VT/performer changes are outside this feature. Native CI's command and strict lint of the changed packages both pass. Release-profile `make test` and Linux/Windows UI tests were not run.

Manual UI checks used a separate `/tmp/RioWorkspaces.app` bundle and `/tmp/rio-workspace-qa-config`, with Alpha (default shell + explicit long-running command in `subdir`), Beta (default shell), a partial-failure recipe, and a missing-root recipe:

- Opened workspace palette through a configured F6 action; searched by typing and selected using arrows/Enter. Visually checked **Open** markers and both horizontal tab switching and vertical panel layout. Default shell started in Alpha/Beta roots; command printed its subdirectory CWD.
- Switched Alpha -> Beta -> Alpha; observed the original Alpha command PID `2944` and terminal contents on return, with no extra tab/panels. Its heartbeat file continued to grow while hidden (21 -> 93 lines). Automated tests also perform repeated switches and check exact PID/terminal Arc identity.
- Selected missing-root and partial-failure recipes. Missing-root left existing tabs intact; partial failure created its working shell and reported the nonexistent program and panel directory.
- Verified the native macOS Paste menu now enters `Alpha` into palette search instead of the underlying PTY, on the final build.
- Closed Alpha's command split through a configured F9 `CloseSplitOrTab`; PID `16555` was gone, remaining shell/tab stayed open. Selecting Alpha again kept the single shell. Closed the whole workspace via F8 `CloseTab`, returned to the original tab, then reopened Alpha: panels were created again with new PID `19184`.
- A deferred test recipe's marker file remained absent on startup and after a config edit/reload. The deferred recipe was never selected.

UI automation intermittently reported ScreenCaptureKit/cgWindowNotFound errors. Successful checks above were visually observed, not inferred from a command invocation. The later attempt to visually inspect a removed-recipe live tab after reload could not be completed because of that capture issue. Native quit-confirmation interaction, tab drag and NativeTab UI were not manually tested; upstream close behavior was inspected and identity/reorder/native rejection are covered in unit tests. Long error diagnostics are wrapped for the existing fixed-width warning card; larger batches remain subject to its upstream 16-line limit.

Next small step: add a focused UI regression harness for config reload and the native menu overlay gate, then consider global cross-window focus if desired.
