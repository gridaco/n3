# Terminal and the Tool Dock

The **Tool Dock** is the shared workspace container. **Animation** and
**Terminal** are its individual panels; the **Tool Dock tab bar** switches between
them. The current placement is below the viewport. The name identifies the
container independently of that placement.

The Tool Dock starts closed. Its tab bar floats at the viewport's lower-left
until a panel opens, then moves into the Tool Dock header. Switching panels
retains the same container and height. Activating the selected tab focuses its
panel. The shared close control dismisses the Tool Dock and returns focus to the
viewport. These are transient workspace operations, outside authored documents
and edit history. The Tool Dock adds no default content padding; each panel
owns its internal spacing.

## Ownership

`src/terminal/` contains the feature within N3's existing application crate:

- `TerminalSession` owns the Alacritty terminal state, dimensions, terminal
  configuration, and native PTY attachment. Output bytes pass through the real
  emulator. Native sessions and deterministic fixtures use the same input and
  emulator boundaries. History is bounded to 1,000 lines; OSC 52 clipboard
  access is disabled.
- `TerminalView` owns egui layout, cell painting, input encoding, pointer/focus
  interaction, and scrollback navigation. It uses N3's theme, monospace font
  family and glyph fallbacks, and the ordinary egui/wgpu rendering path. The
  available cell grid determines rows and columns, including PTY resize events.
- The native host explicitly enables shell launching. Opening Terminal in that
  host starts a login shell in the user home directory on demand (falling back
  to the process directory if no valid home is available). Child-only terminal
  identity and UTF-8 locale defaults preserve explicit environment overrides.
  Its session remains alive while the
  Tool Dock is closed or Animation is selected. Documentation and the workbench
  do not implicitly run the user's shell or startup files.
- The Tool Dock routes focus. Focused Terminal input goes to the terminal rather
  than viewport commands. Ownership persists across held-key releases and
  repeated egui layout passes. Caller-supplied identities keep views separate.

Alacritty types stay inside the terminal feature. Terminal sessions do not enter
an authored document or undo history. Terminal commands run in a local shell and
may have filesystem or process effects; N3's Undo does not reverse those effects.
There is no N3 command parser or application scripting API. A future N3 Console
will own its command semantics independently of the terminal emulator.

## Native I/O and recovery

The native attachment owns PTY I/O, process lifecycle, and wakeups. The view
submits encoded input; process output updates the shared emulator and requests
repaint. Grid changes reach the PTY so applications receive current terminal
sizes. Emulator replies, including cursor-position and device-status responses,
return to the same PTY. Palette and terminal-size queries use the current view's
appearance and dimensions.

Shell startup failure and process exit remain visible session states. Closing
the Tool Dock hides its contents without terminating the shell. Application
shutdown releases the process attachment. Starting a process is explicit; a
read-only sample or input-recording fixture cannot start one implicitly.

## Verification boundaries

The [Terminal user guide](../guide/terminal.md) uses a real controlled `/bin/sh`
PTY with an isolated prompt and disabled profile hooks. The scenario opens the
actual Tool Dock, types a `printf` command through production UI input, and
waits under a bounded deadline for the actual shell output before capturing it.
It never injects the expected response. Asynchronous process waits do not advance
the scenario's explicit replay clock. Light and Dark captures use the normal
documentation pipeline.

Focused application regressions use a deterministic interactive fixture to
assert exact outbound bytes, input ownership, repeated-pass behavior, and
unchanged document history. Native PTY tests independently check process
execution, output, and resizing. Internal workbench cases continue to support
isolated views without requiring a user's shell environment.

Compatibility is established per behavior. ANSI rendering, alternate screen,
cursor/device queries, bracketed paste, control keys, and resize handling are
relevant to terminal applications such as Claude Code and Codex CLI. These
protocol tests do not prove complete compatibility with every TUI or
application-specific keybinding. Input currently uses classic VT/xterm encodings;
the Kitty enhanced keyboard protocol is not negotiated, and Shift+Enter has the
same carriage-return encoding as Enter.

A local read-only check on 2026-10-01 found Claude Code 2.1.126 and Codex CLI
0.159.2. Their `--help` and `--version` commands were inspected without starting
an authenticated session. Codex's help documents `--no-alt-screen` for an inline
mode that preserves scrollback. Full interactive Claude/Codex sessions were not
verified by those checks.

## Reference

The backend/view boundary follows the principle described by
[Zed's terminal view architecture](https://github.com/zed-industries/zed/blob/main/crates/terminal_view/README.md).
N3 owns its egui integration; no Zed implementation was copied.
[alacritty_terminal 0.26.0](https://docs.rs/alacritty_terminal/0.26.0/alacritty_terminal/)
is the terminal engine, distributed under Apache-2.0.
