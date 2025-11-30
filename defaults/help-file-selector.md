# File Selector Help

Press **{help}** or **ESC** to close this help.

## File Selector - Choose or Create Files

### Navigation

| Key | Action |
|-----|--------|
| **Up** / **Down** | Move selection |
| **k** / **j** | Move selection (Vim-style) |
| **Page Up** / **Page Down** | Scroll by page |
| **Home** / **g** | Jump to first file |
| **End** / **Shift+G** | Jump to last file |

### File Operations

| Key | Action |
|-----|--------|
| **Enter** | Open selected file in editor |
| **Ctrl+N** | Create new file (enter path) |
| **Ctrl+D** | Delete selected file and its history |

### File List

- Files are sorted by **recent access** (most recent first)
- Files with unsaved changes are marked with **\***
- Full path is shown for each file

### Creating New Files

1. Press **Ctrl+N**
2. Enter the file path (absolute or relative)
3. Press **Enter** to create and open
   - Parent directories will be created automatically
   - Relative paths are relative to current directory

### Other

| Key | Action |
|-----|--------|
| **{help}** | Show this help |
| **ESC** / **q** / **Ctrl+C** | Quit to shell |

---

**Note:** Keybindings can be customized in `~/.ue/settings.toml`

