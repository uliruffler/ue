# UE Editor - Instructions for Copilot

## Project Overview
**ue** (Uli's Editor) is a terminal-based text editor written in Rust that aims to provide modern text editing features similar to GUI editors like Notepad++, but runs entirely in the terminal. The project is AI-assisted and heavily generated using GitHub Copilot.

**Key Philosophy**: Internal tool with no public APIs. Use `pub(crate)` only when needed for cross-module access.

**Recent Improvements** (December 2023):
- Full UTF-8/Unicode support (German umlauts, emoji, multi-byte characters)
- **New file feature (Ctrl+N)**: Creates untitled documents immediately
  - Pressing `Ctrl+N` or File menu → New creates an untitled file (untitled, untitled-2, etc.)
  - User can start typing immediately without choosing filename
  - When saving (Ctrl+S), dialog prompts for filename and location
  - Untitled undo files automatically cleaned up after save and moved to new location
  - Multiple untitled files supported with unique naming (untitled, untitled-2, untitled-3...)
  - Untitled files stored in `~/.ue/files/` root (not in subdirectories)
- Non-existent file opening: Can open with non-existent filename (e.g., `ue newfile.txt`)
  - File tracked immediately with full undo support
  - Actual file created on disk only when saved (`Ctrl+s`)
- File closing from file selector: `Ctrl+w` to close/untrack files
- Session restoration: Remembers last mode (editor/selector) and file when reopening
- Recent files ordering: Most recently opened file appears at top of file selector
- Bug fixes: File removal path construction, test infrastructure improvements

## Core Features

### Text Editing
- **UTF-8 support**: Full Unicode support including multi-byte characters (e.g., German umlauts "ü", emoji)
  - Cursor position tracked in character indices, not byte offsets
  - Proper handling of character boundaries for all editing operations
- **Line numbering** with configurable gutter width
- **Line wrapping** for long lines
- **Persistent undo/redo** mechanism with file-based history
- **Clipboard operations** (copy, cut, paste) via `arboard` crate
- **Multi-file support** via file selector (no tabs, single file view at a time)
- **Session persistence**: Restores last file, cursor position, and scroll state
- **Auto-save** of editor state (cursor, scroll, undo history)

### Navigation & Selection
- **Cursor navigation**: Arrow keys, `Home`, `End`, `Ctrl+Arrow` (word/paragraph jumps)
- **Text selection**:
  - Line-wise: `Shift+Arrow`
  - Block selection: `Alt+Shift+Arrow` or `Alt+Click-Drag`
  - Word selection: Double-click
  - Triple-click: Select entire line
- **Multi-cursor mode**: `Alt+Up/Down` to add cursors, type on multiple lines simultaneously
- **Mouse support**: Click positioning, drag selection, scrolling (3 lines per wheel event)
- **Go-to line**: `Ctrl+g` enters go-to mode for quick line navigation

### Search
- **Find mode**: `Ctrl+f` enters regex-based search with live highlighting
- **Case-insensitive** by default (pattern wrapped in `(?i)`)
- **Scoped search**: If text is selected when entering find mode, search is limited to selection
- **Find next/previous**: `Ctrl+n` / `Ctrl+p`
- **Search history**: Up to 100 recent searches persisted

### Visual Features
- **Syntax highlighting**: Extensible via `.ue-syntax` files (regex-based patterns)
- **Scrollbar** showing current viewport position
- **Header**: Displays filename, modification status, line/column indicators
- **Footer**: Shows cursor position, selection info, and mode (find, go-to, etc.)
- **Configurable colors**: Header, footer, line numbers via `settings.toml`
- **Cursor shapes**: bar (default), block, or underline

### File Management
- **New file (Ctrl+N)**: Creates untitled documents for quick note-taking
  - Press `Ctrl+N` or select File → New from menu
  - Immediately opens an empty "untitled" buffer (no filename dialog)
  - User can start typing right away
  - Multiple untitled files get unique names: `untitled`, `untitled-2`, `untitled-3`, etc.
  - Untitled files are tracked with full undo/redo support
  - **Saving untitled files**:
    - Press `Ctrl+S` to save
    - Save-as dialog appears asking for filename and location
    - Can navigate directory tree or type path (relative or absolute)
    - After save, untitled undo file (`~/.ue/files/untitled.ue`) is automatically deleted
    - Undo history moved to new file location
    - File removed from recent files list under untitled name
  - **Untitled file storage**:
    - Undo files stored in `~/.ue/files/` root (not in subdirectories)
    - Naming pattern: `untitled.ue`, `untitled-2.ue`, etc.
    - Case-insensitive detection (UNTITLED also treated as untitled)
- **File selector**: Press `Esc` to toggle between editor and file selector
  - Shows all tracked files from `~/.ue/files/**`
  - Sorted by recent usage (via `~/.ue/files.ue`) - most recently opened file at top
  - Shows unsaved changes indicator
  - `Enter` opens file, `Esc` returns to editor
  - `Ctrl+w` closes selected file (removes from tracking)
- **Opening non-existent files**: Can open with filename that doesn't exist (e.g., `ue newfile.txt`)
  - File tracked immediately with full undo support
  - Actual file created on disk only when saved (`Ctrl+s`)
  - Undo history persisted even for unsaved new files
- **Session restoration**: Opening `ue` without arguments restores previous state
  - If quit from editor mode on file A, reopens in editor mode on file A
  - If quit from file selector, reopens in file selector
  - Preserves cursor position, scroll state, and mode
- **Quick exit**: Double-tap `Esc` (within 300ms default) to exit without saving
  - Changes are not lost; state is persisted to `~/.ue/files/`

### Help System
- **Built-in help**: Press `F1` to view context-sensitive help
- **Markdown rendering** via `termimad` crate
- Help files stored in `defaults/` directory

## Architecture

### Module Structure
```
main.rs          Entry point, CLI argument parsing
lib.rs           Test-only module exports (NOT a public API - for integration tests only)
ui.rs            Orchestration layer, main event loop, terminal setup/teardown
editor_state.rs  FileViewerState struct (cursor, selection, scroll, flags)
coordinates.rs   Position calculations, line wrapping, visual ↔ logical mapping
rendering.rs     Screen rendering (header, footer, content, scrollbar)
editing.rs       Text modification, clipboard, save/load, undo application
event_handlers.rs Keyboard/mouse event processing, navigation logic
mouse_handlers.rs Mouse-specific logic (selection, double-click, block mode)
undo.rs          Undo/redo history management, persistence
settings.rs      Configuration loading, keybinding parsing
syntax.rs        Syntax highlighting engine
file_selector.rs File selector UI and logic (includes remove_tracked_file)
find.rs          Find mode logic, search history
help.rs          Help system rendering
session.rs       Session state persistence (last file, mode)
recent.rs        Recent files tracking (most recently used ordering)
double_esc.rs    Double-tap Esc detection
env.rs           Test environment utilities (temp home directory with locking)
```

### Key Design Principles

1. **Single Responsibility**: Each module has one clear purpose
2. **No Public APIs**: Internal tool, not a library
   - All modules use `pub(crate)` for cross-module access
   - `lib.rs` exists solely for integration test infrastructure (re-exports with `pub`)
   - Not intended for external consumption or as a library dependency
3. **Orchestration via ui.rs**: Main event loop coordinates modules; modules don't call each other directly
4. **State-Driven Rendering**: `FileViewerState.needs_redraw` flag controls when screen updates
5. **Testable**: Logic isolated in pure functions where possible; comprehensive test coverage
6. **Persistent State**: Editor state (cursor, scroll, undo) saved to `~/.ue/files/` directory structure

### Data Flow Example
```
User input → ui.rs event loop
           ↓
event_handlers::handle_key_event() / handle_mouse_event()
           ↓
Modify FileViewerState (cursor, selection, etc.)
Set state.needs_redraw = true
           ↓
ui.rs checks needs_redraw
           ↓
rendering::render_screen()
  → Uses coordinates.rs for position calculations
  → Uses syntax.rs for highlighting
           ↓
Screen updated
```

## Important Data Structures

### FileViewerState (editor_state.rs)
Central state container:
- **Position**: `top_line`, `cursor_line`, `cursor_col`
- **Selection**: `selection_start`, `selection_end`, `selection_mode` (Line/Block)
- **Multi-cursor**: `multi_cursors` (Vec of positions)
- **Find**: `find_active`, `find_pattern`, `find_scope`, `last_search_pattern`
- **Flags**: `needs_redraw`, `modified`, `mouse_dragging`
- **Undo**: Reference to `UndoHistory`

### UndoHistory (undo.rs)
Edit tracking:
- **Edit enum**: InsertChar, DeleteChar, SplitLine, JoinLine, etc.
- Persisted to `~/.ue/files/{hash}/undo.json`
- Applied via `apply_undo()` / `apply_redo()` in editing.rs

### Settings (settings.rs)
Loaded from `~/.ue/settings.toml`:
- **Keybindings**: Customizable key mappings (e.g., `quit = "Esc Esc"`)
- **Appearance**: Colors, line number width, cursor shape
- **Behavior**: Tab width, scroll lines, double-tap speed

## Syntax Highlighting

Syntax definitions stored in `~/.ue/syntax/*.ue-syntax`:
- Format: `pattern = "regex" color = "Blue" priority = 10`
- Deployed from `defaults/syntax/` on first run
- Priority determines which color wins for overlapping matches
- Patterns are regex-based (via `regex` crate)

## File Persistence

All editor state stored in `~/.ue/`:
```
~/.ue/
  settings.toml       User configuration
  files.ue            Recent files list (most recent first, one per line)
  last_session        Last active file/mode
  syntax/             Syntax definitions
  files/              Per-file state (mirrors absolute path structure)
    path/to/          Directory structure matching file location
      file.txt.ue     Combined state file (undo history, position, content)
```

Note: The `.ue` files in `files/` directory mirror the absolute path of the original file. For example:
- `/home/user/doc.txt` → `~/.ue/files/home/user/doc.txt.ue`
- Hidden files like `.bashrc` → `~/.ue/files/home/user/.bashrc.ue`

## Testing

- Run tests with `cargo test`
- **Unit tests**: 324 tests per binary (lib + main), use `#[cfg(test)]` modules within each file
- **Integration tests**: 7 tests in `tests/integration_tests.rs`, test full workflows
  - Use `#[serial]` attribute from `serial_test` crate to prevent race conditions
  - Tests run sequentially to avoid environment variable conflicts
- Use `UE_TEST_HOME` environment variable to isolate test state
- Tests should not produce warnings (zero warnings policy)
- Add tests for new features
- Modules with significant test coverage: undo.rs, coordinates.rs, settings.rs, file_selector.rs

## Dependencies

- **crossterm**: Terminal manipulation, events, raw mode
- **clap**: CLI argument parsing
- **serde/toml**: Configuration serialization
- **arboard**: Cross-platform clipboard
- **regex**: Pattern matching for search & syntax highlighting
- **termimad**: Markdown rendering for help
- **serde_json**: Undo history persistence
- **tempfile**: Test utilities (dev-dependency)
- **serial_test**: Sequential test execution for integration tests (dev-dependency)

## Common Workflows

### Adding a New Keybinding
1. Add field to `KeyBindings` struct in `settings.rs`
2. Add default function (e.g., `fn default_my_action() -> String`)
3. Add `#[serde(default = "default_my_action")]` attribute
4. Parse binding in `event_handlers.rs` using `settings.keybindings.my_action_matches()`
5. Update `defaults/settings.toml`
6. Update help files if user-facing

### Adding a New Syntax Definition
1. Create `.ue-syntax` file in `defaults/syntax/`
2. Define patterns with regex, color, priority
3. Add to `default_syntax.rs` deployment list
4. File automatically deployed on first run

### Adding a New Edit Type
1. Add variant to `Edit` enum in `undo.rs`
2. Implement undo logic in `apply_undo()` / `apply_redo()` (editing.rs)
3. Create edit record when performing action
4. Call `state.undo_history.push()` to add to history

## Visual Modes

### Block Selection Mode
- Rectangular text selection across multiple lines
- Activated via `Alt+Shift+Arrow` or `Alt+Click-Drag`
- Zero-width block: Acts as multi-line cursor (insert on all lines)
- Copy/paste/delete operations work on rectangular region
- Lines shorter than selection are partially selected

### Multi-Cursor Mode
- Create multiple independent cursors with `Alt+Up/Down`
- Visual feedback: Blinking block cursor at each position (500ms interval)
- Typing inserts at all cursor positions simultaneously
- Exit with `Esc` or any navigation key

### Find Mode
- Entered with `Ctrl+f`
- Live highlighting as you type
- If text selected: Search limited to selection (scoped search)
- `Enter`: Jump to next match and exit find mode
- `Esc`: Cancel and restore previous search highlights

## Error Handling

- Prefer `Result` and `?` operator
- Avoid `unwrap()` in production code (tests OK)
- Graceful degradation: Missing config files use defaults
- File I/O errors logged but don't crash editor

## Code Style

- Rust 2024 edition
- Run `cargo fmt` before committing
- Build with zero warnings: `#![deny(warnings)]` in main.rs
- Use descriptive variable names
- Keep functions focused and testable
- Comments explain "why", not "what"

## Performance Considerations

- Line wrapping calculated on-demand, not pre-computed for entire file
- Only visible lines rendered (viewport-based)
- Syntax highlighting applied per visible line
- Undo history capped at reasonable size (no explicit limit currently)
- Release build uses LTO and single codegen unit for optimization

## Future Enhancement Areas

- LSP integration for code intelligence
- Multiple viewport splits
- Macros/recording
- Plugin system
- Remote file editing
- Git integration
- More syntax definitions
