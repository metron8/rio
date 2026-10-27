# Save a tab as a new workspace

Added on branch `feat/workspace-capture`, based on workspace MVP commit `0680d3cb7eff3f391c5c56ea85373efd116cdc26`.

1. Arrange ordinary Rio splits and change each pane to its project directory.
2. Open the command palette and select **Save Tab as New Workspace**.
3. Enter a name; press Enter to save or Escape to cancel.
4. The current tab becomes the newly named workspace. Select it in **Open Workspace** to return without restarting processes.
5. After closing the tab, opening that recipe creates its panels again.

The action can also be bound through normal Rio configuration:

```toml
[bindings]
keys = [
  { key = "F6", action = "OpenWorkspacePalette" },
  { key = "F7", action = "SaveWorkspace" },
]
```

Merge keys into existing bindings. The locally installed Rio Workspaces app uses `~/.config/rio-workspaces/config.toml`; an ordinary checkout uses Rio's normal config directory unless RIO_CONFIG_HOME is set.

## What is captured

- A new stable ID, derived from the name. Collisions get numeric suffixes; open workspace IDs removed from config are reserved too. Duplicate display names are allowed.
- Panel directories, read from OS foreground-process CWD when available, then shell integration, then the known launch directory. Unknown or inaccessible directories produce an error; the form and existing config remain available to retry.
- The common ancestor of panel directories as root, with relative paths per panel. Paths are canonicalized before comparison.
- The original explicitly configured program and argv of each pane, recorded before shell integration injects arguments. A default-shell pane remains a default-shell pane. Commands typed interactively inside a shell, shell history and arbitrary foreground jobs are not reconstructed.
- The nested split topology. An optional `split-from` field names the zero-based index of an earlier panel to split; absent retains the MVP's previous-panel behavior. Only backward references are accepted.

For example, a right split followed by a down split of the original left panel is represented by:

```toml
[[workspaces.panels]]
[[workspaces.panels]]
split-from = 0
[[workspaces.panels]]
split-from = 0
split-down = true
```

Capture does not save divider ratios, window geometry, scrollback, selection, shell variables, job memory or the currently focused pane. Reopening creates equal splits through Rio's normal layout APIs and focuses the last created pane. When a referenced pane fails to launch, surviving panels fall back to the current successful pane; the original launch failure is reported and successful processes are kept.

## Persistence and failure behavior

**Save Tab as New Workspace** always creates a new recipe. Saving a tab already attached to a workspace gives the live tab the new ID/name; its previous recipe remains in the configuration and can be opened separately. It never overwrites another recipe just because its name matches.

The file is reread and validated at save time. `toml_edit` preserves unrelated settings and comments (formatting inside a converted inline workspace array may change). A sibling advisory lock serializes saves from Rio windows; a temporary file in the same directory is flushed and atomically persisted. Existing permissions and config symlinks are preserved. A final content check detects intervening editor changes before replacement; editors ignoring the advisory lock can still race the final rename. Read-only files, invalid configuration, lock contention and I/O errors are reported. Tab identity changes only after successful persistence.

The current process graph is untouched by capture/save. Config reload is explicitly requested after persistence, including for symlink targets outside the watched directory. Other windows receive the new recipe without starting commands. The existing palette/IME/native menu input gate owns the name field, so typed or pasted names do not reach the PTY.

## Verification

Commands:

```sh
cargo fmt --all -- --check
cargo test -p rio-backend -p rioterm workspace --locked
cargo clippy -p rio-backend -p rioterm --all-targets --all-features --no-deps --locked -- -D warnings
cargo test --features wgpu --locked
cargo build -p rioterm --release --locked
```

The workspace filter passed 19 tests. The full debug/wgpu run passed **1430 tests**, 0 failures, 11 upstream ignored tests, across 39 result groups. Strict lint of modified packages passed. No VT or GPU source changes were needed.

New automated cases cover comments/settings/permission/symlink preservation, new config creation, Unicode names, collision handling, invalid/readonly config and competing writes, inline arrays, backward split-reference validation, save-form input, unknown directories, and nested split capture -> file save -> config reload -> topology replay. The topology test confirms capture/save preserve live route IDs and focus; existing real-PTY tests cover switching, exact argv/CWD and partial launch failure.

A pre-existing incomplete native build artifact initially caused `glslang-sys` to fail to find its static library. `cargo clean -p glslang-sys` and the repeated full run resolved it; no dependency version was changed to work around it.

Manual macOS QA uses a separate app and RIO_CONFIG_HOME under /tmp, preserving the user's working app and configuration. Through assigned keys, created a second tab, changed CWD, made a right split and then split the left branch down, entered `Captured Rio QA` in the Save form and submitted. The resulting file contains all three panels with `split-from = 0` for both branches, correct root and original configuration comments. Native ScreenCaptureKit intermittently returned a stale frame for this debug window; this does not establish a successful visual layout/form roundtrip. Additional release UI results are recorded after verification below.

Release UI verification completed: launched a distinct release app with the same isolated config, observed `Captured Rio QA` in the palette after a new application launch, and opened its three-panel layout (left split vertically, right full height). Opened Save via F7, visually checked the name form and pasted `Release Saved QA`, submitted, and observed the live tab renamed with all three shells retained. Closed it through standard CloseTab, selected the new recipe from the palette, and visually verified the same nested layout recreated. The original recipe remained available. Release build completed successfully (2m40s). Installed-app updates preserve the user's configuration and keep the preceding app/config in a local backup.
