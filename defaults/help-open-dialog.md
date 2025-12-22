# Open Dialog - Directory Tree Browser

Press **Ctrl+O** to open the directory tree browser.

## Features

The open dialog provides a full-window directory tree view for browsing and opening files from the filesystem.

### Navigation

| Key | Action |
|-----|--------|
| **Up** / **k** | Move selection up one line |
| **Down** / **j** | Move selection down one line |
| **Left** / **h** | Collapse directory or move to parent |
| **Right** / **l** | Expand directory and move to first child |
| **Enter** | Open selected file or toggle directory |
| **Tab** | Switch focus to input field |
| **.** (period) | Toggle hidden files visibility |
| **Esc** | Cancel and return to editor |

### Left/Right Behavior

- **Left**: If on an expanded directory, collapses it. If already collapsed or on a file, moves to the parent directory.
- **Right**: If on a directory, expands it and moves to the first child. Does nothing on files.

### Input Field

Below the tree is an input field where you can type or paste a file path directly.

| Key | Action |
|-----|--------|
| **Tab** | Switch focus back to tree |
| **Enter** | Open the path typed in the input field |
| **Esc** | Clear input and return to tree (or cancel if empty) |
| **Ctrl+V** | Paste from clipboard |
| Any character | Automatically switches focus to input field |

### Tree Display

- Directories are shown with **▶** (collapsed) or **▼** (expanded) indicators
- Files and directories are sorted alphabetically (case-insensitive)
- Directories are grouped before files
- Tree-style visualization with indentation and branch characters
- Current file is pre-selected when opening the dialog

### Hidden Files

By default, hidden files (starting with `.`) are not shown. Press **.** (period) to toggle their visibility.

## Integration

The open dialog:
- Opens to the current file's directory
- Pre-selects the current file in the tree
- Updates recent files list when a file is selected
- Returns to the editor with full redraw after selection
- Preserves editor state (cursor position, undo history) when opening

## Access

- **Menu**: File → Open Dialog...
- **Keybinding**: Ctrl+O (configurable in settings.toml)

## Settings

Add or modify in `~/.ue/settings.toml`:

```toml
[keybindings]
open_dialog = "Ctrl+o"
```

