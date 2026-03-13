# UE Editor — Copilot Instructions

## What This Is

**ue** is a terminal text editor in Rust (edition 2024). Single binary, no public library API.
All cross-module visibility uses `pub(crate)`. `lib.rs` exists **only** for integration tests.

---

## Crate Layout

```
src/
  main.rs            CLI entry (clap), path resolution, calls ui::show()
  lib.rs             Re-exports every module as pub — integration tests ONLY
  ui.rs              Event loop, terminal setup/teardown, orchestration hub
  editor_state.rs    FileViewerState<'a> — the single source of truth for all state
  coordinates.rs     Logical ↔ visual position math; word-wrap geometry; Unicode widths
  rendering.rs       Writes to stdout via crossterm queue!/execute!
  editing.rs         Mutates lines[], clipboard, calls undo push, save to disk
  event_handlers.rs  Translates crossterm KeyEvent → state mutations / editing calls
  mouse_handlers.rs  Mouse-specific events (click, drag, double-click, wheel)
  find.rs            Find/replace logic, pattern→regex, history, scoped search
  undo.rs            Edit enum, UndoHistory serialisation (serde_json to ~/.ue/)
  settings.rs        Settings + KeyBindings structs, TOML load, key-match helpers
  syntax.rs          Regex-based highlighting engine; stack for embedded languages
  menu.rs            MenuBar / MenuAction enum; drop-down menu rendering + input
  open_dialog.rs     Full-screen directory-tree dialog (Open / Save As)
  markdown_renderer.rs  MarkdownRenderer trait + PulldownRenderer (default)
  session.rs         last_session file (editor vs selector mode, last open file)
  recent.rs          files.ue MRU list (most recently opened first)
  double_esc.rs      Double-tap Esc within N ms → quit
  default_syntax.rs  Embeds defaults/syntax/* and deploys to ~/.ue/syntax/ on first run
  env.rs             UE_TEST_HOME isolation + mutex for serial tests
  help.rs            Help file deployment with keybinding substitution
```

---

## Central Data Flow

```
crossterm event
      │
      ▼
ui.rs  editing_session()
      │  owns: lines: Vec<String>   ← the document
      │  owns: state: FileViewerState<'a>
      │
      ├─► event_handlers::handle_key_event(state, lines, …)
      │         └─► editing::handle_editing_keys / save_file / etc.
      │         └─► find::handle_find_key / …
      │         └─► sets state.needs_redraw = true
      │
      └─► if state.needs_redraw → rendering::render_screen(state, lines, stdout)
                └─► coordinates.rs  (wrap geometry, visual positions)
                └─► syntax::highlight_line()
```

**Rule**: modules do not call each other directly. All coordination flows through `ui.rs`.
`event_handlers` may call `editing`, `find`, `undo` — but never `rendering` or `ui`.

---

## Core Structs

### `FileViewerState<'a>` (editor_state.rs)

This is the god-object. Every piece of mutable editor state lives here.
Lifetime `'a` is bound to `&'a Settings` (settings outlive the state).

Key field groups:

```rust
// Viewport & cursor (0-based character indices, not bytes)
top_line: usize                    // first visible logical line
top_line_visual_offset: usize      // sub-row offset for wrapped-line scrolling
cursor_line: usize                 // absolute logical line
cursor_col: usize                  // character index within the line
desired_cursor_col: usize          // "sticky" column for ↑/↓ through short lines

// Selection
selection_start: Option<Position>  // Position = (usize, usize) = (line, col)
selection_end:   Option<Position>
selection_anchor: Option<Position> // fixed point when Shift+arrow extends
block_selection: bool              // true = rectangular / column selection

// Multi-cursor (Alt+↑/↓)
multi_cursors: Vec<Position>
cursor_blink_state: bool           // toggled every 500 ms

// Redraw control
needs_redraw: bool                 // full screen redraw
needs_footer_redraw: bool          // cheaper: only footer bar

// Find / replace
find_active: bool
find_regex_mode: bool              // true=regex, false=wildcard (* ?)
find_pattern: String
find_cursor_pos: usize             // char index inside the find input
find_scope: Option<((usize,usize),(usize,usize))>  // restrict search to selection
replace_active: bool
replace_pattern: String

// Pending deferred action from menu → handled by ui.rs on next loop iteration
pending_menu_action: Option<MenuAction>

// Multi-instance sync
last_save_time: Option<Instant>    // prevents reload loop after our own save

// Markdown preview
markdown_rendered: bool
rendered_lines: Vec<String>        // ANSI-decorated display lines
```

### `UndoHistory` (undo.rs)

Event-sourced undo. Serialised as JSON to `~/.ue/files/<mirrored-path>.ue`.

```rust
pub struct UndoHistory {
    pub edits: Vec<Edit>,   // the log
    pub current: usize,     // pointer; edits[current..] are "future" (redoable)
    pub saved_at: usize,    // current value when file was last saved → drives `modified`
    pub file_content: Option<Vec<String>>,  // snapshot at history-load time
    pub cursor_line / cursor_col / scroll_top  // restored on open
    pub find_history: Vec<String>   // persisted per-file search history
    pub replace_history: Vec<String>
}

pub enum Edit {
    InsertChar { line, col, ch },
    DeleteChar { line, col, ch },
    SplitLine  { line, col, before, after },   // Enter key
    MergeLine  { line, first, second },         // Backspace at line start
    InsertLine { line, content },
    DeleteLine { line, content },
    ReplaceLine { line, old_content, new_content },
    DeleteWord { line, col, text, forward },
    DragBlock  { before, after, source_start, source_end, dest, copy },
    CompositeEdit { edits: Vec<Edit>, undo_cursor: CursorState },
}
```

**Undo invariant**: `edits[0..current]` are applied; undo decrements `current`; redo increments it.
`apply_undo()` / `apply_redo()` in `editing.rs` replay edits in reverse/forward order against `lines`.

### `Settings` / `KeyBindings` (settings.rs)

Loaded once from `~/.ue/settings.toml`; all keybinding fields accept human-readable strings
like `"Ctrl+s"`, `"Alt+Shift+Down"`. Each action exposes a `_matches(code, modifiers)` method:

```rust
settings.keybindings.save_matches(&code, &modifiers)   // → bool
```

When adding a new binding: add a field + `fn default_*() -> String` + `#[serde(default="...")]`.

---

## Coordinate System

**Logical** position: `(line: usize, col: usize)` where `col` is a **character index** (not bytes, not visual columns).

**Visual** position: terminal row/column after word-wrap expansion. A single logical line can span multiple visual rows.

Key functions in `coordinates.rs`:

| Function | Purpose |
|---|---|
| `visual_width(s, tab_width)` | Terminal columns a string occupies |
| `visual_width_up_to(s, char_idx, tab_width)` | Columns up to char N |
| `visual_col_to_char_index(line, vcol, tab_width)` | Visual col → char index |
| `calculate_word_wrap_points(line, width, tab_width)` | Break-point char indices |
| `calculate_wrapped_lines_for_line(line, width, tab_width)` | Count of visual rows |
| `calculate_cursor_visual_line(state, lines)` | Cursor's visual row in viewport |

Unicode rule: **always use `s.chars().count()` for length; byte indexing requires `char_index_to_byte_index()`** (defined in `editing.rs`).

---

## Rendering Pipeline

`rendering::render_screen(state, lines, stdout)` is the only place that writes to the terminal.
It uses `crossterm::queue!` to batch output, then flushes once.

Rendering layers (top to bottom):
1. **Menu bar** (if `state.menu_bar.is_any_open()`)
2. **Header bar** — filename (shortened via `shorten_path_for_display`), modified `*`, line/col
3. **Line content** — visible lines only (`top_line` … viewport height)
   - Line numbers drawn from gutter
   - Each character coloured by `syntax::highlight_line()` (byte-range → `Color`)
   - Word-wrap produces multiple visual rows per logical line
   - Selection and find-match highlights applied on top
4. **Vertical scrollbar** (right edge)
5. **Horizontal scrollbar** (if line wider than viewport)
6. **Footer** — cursor pos, selection info, mode prompt (find, goto, replace)

`needs_footer_redraw` skips steps 1–5; useful for cursor-only movement.

---

## Syntax Highlighting Engine (syntax.rs)

Global (thread-local) **syntax stack**: allows embedded languages (e.g., code fences in Markdown push a Rust highlighter, then pop back).

```
syntax::set_current_file(path)  // selects top-level syntax from file extension
syntax::push_syntax(ext)        // push embedded language
syntax::pop_syntax()            // restore previous
syntax::highlight_line(line)    // → Vec<(start_byte, end_byte, Color)>
```

`.ue-syntax` file format (in `~/.ue/syntax/`):
```
pattern = "\\bfn\\b"   color = "Yellow"   priority = 10
```

Priority resolves overlapping matches (higher wins). `switch_to`/`switch_back` fields enable the embedding stack.

---

## Find / Replace (find.rs)

- Pattern is either **regex** (`find_regex_mode = true`) or **wildcard** (`* = .*`, `? = .`).
- Always wrapped in `(?i)` for case-insensitive matching.
- `find_scope`: if set, only `lines[scope.start..=scope.end]` are searched.
- Multi-line search: user types `\n` literal → expanded to real `\n`, lines joined.
- `pattern_to_regex(pattern, regex_mode) → Result<Regex>` is the single entry-point.
- Wrap-warning: first time the search wraps around, a "wrapped" message is shown; the second press executes.

---

## File Persistence Layout

```
~/.ue/
  settings.toml                   user config (created from defaults on first run)
  files.ue                        MRU list — one absolute path per line, most recent first
  last_session                    JSON: {mode: "editor"|"selector", file: "/path"}
  syntax/                         deployed .ue-syntax definitions
  files/                          undo / state files — mirrors absolute paths
    home/user/project/main.rs.ue  JSON UndoHistory for /home/user/project/main.rs
    untitled.ue                   untitled buffer (no subdirectory)
    untitled-2.ue                 second untitled buffer
```

`UndoHistory::history_path_for(filename) → PathBuf` performs the mapping.

---

## Multi-Instance Synchronisation (ui.rs)

Two constants drive the undo-file change detection:

```rust
const UNDO_FILE_CHECK_INTERVAL_MS: u64 = 150;  // poll interval
const SAVE_GRACE_PERIOD_MS:        u64 = 200;  // ignore changes right after our own save
```

If another `ue` instance modifies the undo file, the current instance detects the mtime change and prompts the user to reload or keep their version (`show_undo_conflict_confirmation`).

---

## Markdown Preview (markdown_renderer.rs)

Trait-based, swappable at runtime:

```rust
pub(crate) trait MarkdownRenderer: Send + Sync {
    fn render(&self, markdown: &str, term_width: usize) -> Vec<String>;
}
```

Default: `PulldownRenderer` (pulldown-cmark). Legacy: `TermimadRenderer`.
When `state.markdown_rendered = true`, `rendering.rs` displays `state.rendered_lines` instead of `lines`.
Editing is always against the raw `lines`; the rendered view is read-only.

---

## Menu System (menu.rs)

`MenuBar` holds `Vec<Menu>`. Each `Menu` has `Vec<MenuItem>`.

```rust
enum MenuItem {
    Action    { label, action: MenuAction },
    Checkable { label, action, checked, enabled },
    Separator,
}
```

`handle_menu_key(state.menu_bar, key_event) → (Option<MenuAction>, needs_redraw)`.

Actions that require `ui.rs` context (e.g., open new file) are not executed inline; instead they
set `state.pending_menu_action = Some(action)` and `ui.rs` handles them on the next loop pass.

---

## Open / Save-As Dialog (open_dialog.rs)

Full-screen overlay. Two focus modes: `Tree` (directory navigation) and `Input` (manual path entry).
Returns `OpenDialogResult::Selected(PathBuf)` or `Cancelled`.

Used for: **Ctrl+O** (open), **Ctrl+S on untitled file** (save-as).

---

## Programming Conventions

| Convention | Rule |
|---|---|
| Visibility | `pub(crate)` for everything. `pub` only in `lib.rs` for test re-exports. |
| Unicode | All `col` fields are **char indices**. Use `.chars().count()`, never `.len()`. |
| Warnings | `#![deny(warnings)]` in `main.rs`. Zero warnings required. |
| Error handling | `Result<T, E>` + `?`. No `unwrap()` in production paths. |
| Redraw | Always set `state.needs_redraw = true` or `state.needs_footer_redraw = true` after state changes. |
| New edit type | Add `Edit` variant → implement in `apply_undo`/`apply_redo` → push `CompositeEdit` if grouping needed. |
| New keybinding | field in `KeyBindings` + `default_*()` fn + `#[serde(default)]` + `_matches()` method + update `defaults/settings.toml`. |
| Tests | Unit tests in `#[cfg(test)]` blocks inside each file. Integration tests in `tests/` use `#[serial]` and `env::set_temp_home()`. |

---

## Dependencies

| Crate | Used for |
|---|---|
| `crossterm` | Raw mode, alternate screen, mouse capture, ANSI colour output |
| `clap` (derive) | CLI argument parsing |
| `serde` / `toml` | Settings de/serialisation |
| `serde_json` | UndoHistory persistence |
| `arboard` | Cross-platform clipboard (lazy-initialised via `OnceLock<Mutex<…>>`) |
| `regex` | Find patterns and syntax highlighting |
| `unicode-width` | Terminal column width of Unicode chars (`UnicodeWidthChar`) |
| `pulldown-cmark` | Markdown → ANSI rendering |
| `termimad` | Legacy Markdown renderer (still available, not default) |
| `tempfile` | Test temp directories (dev) |
| `serial_test` | Sequential integration tests (dev) |
