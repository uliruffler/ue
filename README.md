# ue - Simple Terminal File Viewer

A terminal-based file viewer with configurable key bindings.

## Features

- Page-by-page file viewing
- **Cursor navigation** with arrow keys
- **Syntax highlighting** using Vim syntax files
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
- **Ctrl+Q** (configurable): Quit the application

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

## Configuration

The application reads key bindings from `~/.ue/settings.toml`. If this file doesn't exist, it will be created automatically with default settings on first run.

The default configuration is stored in `settings.toml` in the repository and is embedded into the binary at compile time.

### Default Configuration

```toml
[keybindings]
next_page = ["space", "n"]
quit = "Ctrl+q"
copy = "Ctrl+c"
close = "Ctrl+w"
```

### Configuration Options

#### `[keybindings]`

- **`next_page`**: Array of keys that advance to the next page
  - Example: `["space", "n"]`
  - Supported: any single character or "space"

- **`quit`**: Key combination to quit the application
  - Format: `"[Modifier+]Key"` (e.g., `"Ctrl+q"`, `"Alt+x"`, `"Ctrl+Shift+q"`)
  - Modifiers: `Ctrl` (or `Control`), `Alt`, `Shift`
  - Can combine multiple modifiers with `+`

- **`copy`**: Key combination to copy selected text to clipboard
  - Format: Same as `quit`
  - Default: `"Ctrl+c"`

- **`close`**: Key combination to close current file and move to next
  - Format: Same as `quit`
  - Default: `"Ctrl+w"`

### Example Custom Configuration

```toml
[keybindings]
next_page = ["space", "n", "j"]
quit = "Ctrl+Shift+x"
copy = "Ctrl+y"
close = "Ctrl+d"
```

This would:
- Allow Space, 'n', or 'j' to advance to the next page
- Require Ctrl+Shift+X to quit
- Use Ctrl+Y to copy selected text
- Use Ctrl+D to close current file and move to next

## Syntax Highlighting

The editor supports syntax highlighting using simplified Vim syntax files. See [SYNTAX.md](SYNTAX.md) for details.

Quick setup:
1. Create directory: `~/.ue/syntax/`
2. Add `.vim` files named after extensions (e.g., `rs.vim` for Rust, `py.vim` for Python)
3. Enable in `~/.ue/settings.toml`: `enable_syntax_highlighting = true` (default)

Sample syntax files for Rust and Python are automatically created in `~/.ue/syntax/` on first run.

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

