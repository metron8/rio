# Project workspaces: investigation and design

## Source and development baseline

Upstream: https://github.com/raphamorim/rio. Personal fork: https://github.com/metron8/rio.
Branch: `feat/project-workspaces`. Base: **59203ccd5fa1e6b760a738c1ad653bb29f2f3465**.

Inspected on 2026-10-21. GitHub connector and `gh auth status` both identify `metron8`.
All pages of personal repositories and a fork-inclusive repository search contained no Rio fork; created one with `gh repo fork raphamorim/rio --clone=false`.
`origin` points to the personal fork, `upstream` to raphamorim/rio. Checkout is a sibling of the existing sheepshead experiment; its untracked files were preserved untouched.

Latest published release is [v0.5.28](https://github.com/raphamorim/rio/releases/tag/v0.5.28), published 2026-09-17, SHA `37624389e8dde79ba25a8147a5e809c01d16d1c9`.
Main is two commits ahead, both CoreText font-face fixes for macOS. Chose main to include those fixes, with no intervening terminal ownership changes. All paths below were inspected at this base, not inferred from older Rio versions.

No `AGENTS.md` or `CONTRIBUTING` files exist in the checkout, nor applicable ancestor AGENTS files. Read root README, Cargo/toolchain/config/rustfmt files, Makefile, `.github/workflows/test.yml`, and PTY README. Follow existing Rust style and MIT headers. Makefile prescribes formatting, clippy all targets/features, and release tests; native CI runs clippy all targets/features and `cargo test --features wgpu`.

## Verified module map

Links are relative to this document. For the original implementation, use the pinned base SHA above.

| Concern | Code | Ownership / reusable behavior |
| --- | --- | --- |
| Native windows / application events | [application.rs](../frontends/rioterm/src/application.rs), [router](../frontends/rioterm/src/router/mod.rs), [window layer](../rio-window/src/platform_impl/macos/mod.rs) | Router owns routes keyed by WindowId; each Route has RouteWindow with Screen. NativeTab creates another native window, unlike the default Tab mode. |
| Tabs / focus | [context manager](../frontends/rioterm/src/context/mod.rs), [screen](../frontends/rioterm/src/screen/mod.rs) | Screen owns ContextManager; manager owns SmallVec of ContextGrids and selected index/route; switching never reconstructs contexts. |
| Splits / ordered panels | [layout](../frontends/rioterm/src/layout/mod.rs), [layout tests](../frontends/rioterm/src/layout/compute_tests.rs) | Each ContextGrid is a tab, owns Contexts keyed by Taffy NodeId; tree traversal provides panel order and selected node. Existing split_right/down, resizing and removal APIs. |
| PTY / child processes | [Context and Drop](../frontends/rioterm/src/context/mod.rs), [performer](../rio-vt/src/performer/mod.rs), [PTY spawn](../teletypewriter/src/unix/mod.rs), [child lifecycle](../teletypewriter/src/unix/child.rs) | Context owns messenger, terminal Arc, child terminator and IO-thread handle. Machine owns PTY, reads/writes on worker. Drop sends Shutdown and synchronous hangup; child teardown escalates/reaps using existing handles. No workspace process supervisor needed. |
| CWD / shell integration | [context](../frontends/rioterm/src/context/mod.rs), [shell integration](../frontends/rioterm/src/shell_integration/mod.rs), [PTY spawn](../teletypewriter/src/unix/mod.rs) | Ordinary new tabs/splits can inherit foreground CWD. Explicit spawn accepts working directory and program/args. forkpty ignores CWD and cannot report exec failure synchronously: workspace launches use existing spawn path. Integration injects OSC 7 for supported shells. |
| Config / live reload | [backend config](../rio-backend/src/config/mod.rs), [file watcher](../frontends/rioterm/src/watcher.rs), [application reload](../frontends/rioterm/src/application.rs) | Serde TOML, defaults, try_load and warning overlay; RIO_CONFIG_HOME isolates test config. Reload updates recipes only, never opens one. |
| Actions / keyboard / overlays | [bindings](../frontends/rioterm/src/bindings/mod.rs), [palette](../frontends/rioterm/src/renderer/command_palette.rs), [router input](../frontends/rioterm/src/router/mod.rs), [assistant](../frontends/rioterm/src/renderer/assistant.rs) | Reuse fuzzy matching, keyboard/IME routing, scrolling, mouse selection, warning overlay and standard key bindings. Extend palette list mode; no GPU/VT changes. |
| Existing persistence | [config](../rio-backend/src/config/mod.rs), [shell integration](../frontends/rioterm/src/shell_integration/mod.rs) | Config persists descriptions. Shell integration writes support scripts; terminal scrollback/layout are in-memory. No application session restore or running-process serialization found in inspected paths. |
| Relevant tests | [context unit tests](../frontends/rioterm/src/context/mod.rs), [palette tests](../frontends/rioterm/src/renderer/command_palette.rs), [config tests](../rio-backend/src/config/mod.rs), [child tests](../teletypewriter/src/unix/child.rs) | Existing dead_pty contexts permit deterministic headless focus/identity tests. Real PTY tests cover child exit, shutdown and failed spawn descriptor cleanup. |

## Workspace model and lifecycle

A workspace is one regular Rio tab containing ordered split panels. Add optional stable workspace_id metadata to ContextGrid, so it follows tab moves and disappears naturally when a tab closes. No external map of mutable tab indices, no daemon or separate terminal ownership.

Scope for MVP: unique within each window. A short user question was raised about global uniqueness; absent an answer, use the least complex per-window model. Default Tab navigation is supported. NativeTab mode needs explicit rejection with a helpful instruction rather than silently creating hidden internal tabs.

Config declaration -> explicit palette selection -> look up existing ID -> focus if found, otherwise validate root and launch panels in order -> register first successful panel as a tab -> attach other successful panels using existing splits. Report failures together; keep successes. If all fail, no workspace is registered and the user can retry. Partial failures are not retried on repeat selection, avoiding duplicate processes. Normal tab/split close and child-exit paths drop contexts. Whole-tab close closes the workspace; closing a split leaves the others alive. Last window close / quit retains upstream confirmation behavior.

Recipe edits apply to the next opening after close, never to live panels. Removed recipes remain selectable while their tabs are alive. No commands at startup/reload; no automatic restore after exit. Commands are program plus argv, passed directly to existing PTY spawn. Shell syntax requires an explicitly configured shell and `-c` argument.

## Format and implementation steps

Use top-level `[[workspaces]]` TOML with `id`, `name`, `root`, and ordered `[[workspaces.panels]]`. Panel fields: optional `working-dir`, optional `command = { program, args }`, and `split-down` (default false/right). First successful panel fills the tab; split orientation applies to later successful panels.

Validate unique IDs, nonblank names/programs, at least one panel, absolute or ~/ root, relative panel paths without parent traversal, no NULs, and unknown nested keys. Directory existence is checked at explicit opening, allowing an unmounted project to remain in valid config. No implicit variable or command expansion.

1. Add config model, validation and roundtrip/error tests.
2. Add stable tab metadata and launch/focus helpers; reuse PTY lifecycle.
3. Extend existing palette and add bindable OpenWorkspacePalette action.
4. Test no duplicates, reorder/removal, process survival, partial/all failures and config reload.
5. Build/lint/test following project rules; manually check macOS palette, focus, split close and whole-tab close.

## Baseline verification

macOS arm64, Xcode selected at `/Applications/Xcode.app/Contents/Developer`, accepted Xcode license; installed pinned Rust 1.96.1 with rustfmt/clippy (`rustup toolchain install 1.96.1 --profile minimal --component rustfmt --component clippy`). Dependencies fetched from Cargo.lock. No Homebrew libraries needed for the native macOS build.

`cargo build -p rioterm --locked` completed from a new target directory in 1m02s. Log: `/tmp/rio-base-build.log` (local, not committed).
Ran unmodified binary with `RIO_CONFIG_HOME=/tmp/rio-workspace-base-config target/debug/rio`. For native UI access, copied the same binary into `/tmp/RioWorkspaceBase.app/Contents/MacOS/rio` with Rio's Info.plist and a test bundle ID. In that window entered `printf "RIO_BASE_OK\n"`; visually verified output and next shell prompt. User config was not changed. The test bundle is external to the source tree.

Implementation verification and user instructions: [workspaces.md](workspaces.md).
