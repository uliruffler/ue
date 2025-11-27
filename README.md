# ue - Simple Terminal File Viewer

A terminal-based file viewer with configurable key bindings.

## Features

- Page-by-page file viewing
- **Cursor navigation** with arrow keys
- Configurable key bindings via TOML configuration file
- Automatic configuration file creation with sensible defaults

## Navigation

### Cursor Movement

- **Arrow Keys**: Move cursor up, down, left, right
- **Home**: Move cursor to start of line
- **End**: Move cursor to end of line
- **PageUp**: Scroll up one page
- **PageDown**: Scroll down one page
- **Space** or **n**: Move to next file (if multiple files are opened)
- **Ctrl+W** (configurable): Close current file and move to next
- **Esc** (configurable): Open file selector (after configurable delay if not double-pressed)
- **Esc Esc** (double press, configurable): Quit the application immediately
  - Press Esc twice quickly (default: within 300ms) in the editor to quit instantly
  - Single Esc waits for the timeout (default: 300ms), then opens file selector if no second press
  - Speed is configurable via `double_tap_speed_ms` setting

The current line is highlighted with a `>` marker on the left.

### Text Selection

Hold **Shift** while moving the cursor with arrow keys to select text:

- **Shift + Arrow Keys**: Extend selection
- **Shift + Home/End**: Select to start/end of line
- **Shift + PageUp/PageDown**: Select multiple pages

Selected text is highlighted with reverse video (inverted colors). Release Shift and move without it to clear the selection.

The header shows `[SELECTING]` when text is selected.

### Copy to Clipboard

- **Ctrl+C** (configurable): Copy selected text to clipboard

To copy text:
1. Hold Shift and use arrow keys to select the text you want
2. Press Ctrl+C to copy the selected text to the clipboard
3. The selection remains active so you can continue selecting or paste elsewhere

### Find/Search

- **Ctrl+F** (configurable): Open find mode to search for text using regular expressions
- **F3** (configurable): Find next occurrence
- **Shift+F3** (configurable): Find previous occurrence

**Search within selection**: If you have text selected when you press Ctrl+F, the search will be limited to only the selected range. This is useful for searching within a specific section of the file.

To search within a selection:
1. Hold Shift and use arrow keys to select the text range you want to search in
2. Press Ctrl+F to enter find mode (the search will be scoped to your selection)
3. Type your search pattern (supports regular expressions)
4. Press Enter to find the first match within the selection
5. Use F3/Shift+F3 to navigate through matches (these will search the entire file, not just the selection)

## Configuration

The application reads key bindings from `~/.ue/settings.toml`. If this file doesn't exist, it will be created automatically with default settings on first run.

The default configuration is stored in `settings.toml` in the repository and is embedded into the binary at compile time.

### Default Configuration

```toml
# Double tap speed in milliseconds for Esc Esc quit (default: 300)
double_tap_speed_ms = 300

[keybindings]
quit = "Esc Esc"
file_selector = "Esc"
copy = "Ctrl+c"
paste = "Ctrl+v"
cut = "Ctrl+x"
close = "Ctrl+w"
save = "Ctrl+s"
undo = "Ctrl+z"
redo = "Ctrl+y"
```

Note: `quit = "Esc Esc"` enables double-press detection. The `double_tap_speed_ms` setting controls how quickly you need to press Esc twice to quit (default 300ms). Single Esc waits for the timeout before opening the file selector.

### Configuration Options

#### General Settings

- **`double_tap_speed_ms`**: Time window (in milliseconds) for detecting double Esc press
  - Default: `300` (300 milliseconds)
  - Range: Any positive number (recommended: 200-500)
  - Lower values require faster double-tap, higher values are more forgiving

#### `[keybindings]`

- **`quit`**: Key combination to quit the application
  - Format: `"[Modifier+]Key"` or `"Key Key"` for double-press detection
  - Default: `"Esc Esc"` (press Esc twice within configured time)
  - Examples: `"Ctrl+q"`, `"Alt+x"`, `"Esc Esc"`
  - Double-press format: `"Key Key"` (same key twice, space-separated)

- **`file_selector`**: Key combination to open the file selector
  - Format: Same as other keybindings
  - Default: `"Esc"`

- **`copy`**: Key combination to copy selected text to clipboard
  - Format: Same as `quit`
  - Default: `"Ctrl+c"`

- **`close`**: Key combination to close current file and move to next
  - Format: Same as `quit`
  - Default: `"Ctrl+w"`

### Example Custom Configuration

```toml
[keybindings]
quit = "Ctrl+q"
file_selector = "Ctrl+o"
copy = "Ctrl+Shift+c"
paste = "Ctrl+Shift+v"
close = "Ctrl+d"
save = "Ctrl+Shift+s"
```

This would:
- Use Ctrl+Q to quit (single keypress instead of double Esc)
- Use Ctrl+O to open file selector
- Use Ctrl+Shift+C to copy selected text
- Use Ctrl+Shift+V to paste
- Use Ctrl+D to close current file
- Use Ctrl+Shift+S to save


## Usage

```bash
ue <file1> [file2] [file3] ...
```

### Examples

View a single file:
```bash
ue myfile.txt
```

View multiple files:
```bash
ue file1.txt file2.txt file3.txt
```

## Building

```bash
cargo build --release
```

## License

See LICENSE file for details.

