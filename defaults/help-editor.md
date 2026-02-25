# Editor Help

Press **{help}** or **ESC** to close this help.

## Navigation

| Key | Action |
|-----|--------|
| **Arrow Keys** | Move cursor |
| **Alt+Arrow Keys** | Scroll viewport (vertical & horizontal) without moving cursor |
| **Home** / **End** | Jump to start/end of line |
| **Ctrl+Home** / **Ctrl+End** | Jump to start/end of file |
| **Page Up** / **Page Down** | Scroll by page |
| **{goto_line}** | Go to line |

## Editing

| Key | Action |
|-----|--------|
| **Type** | Insert text |
| **Enter** | New line |
| **Backspace** | Delete character before cursor |
| **Delete** | Delete character at cursor |
| **Tab** | Insert spaces (configurable width) |
| **{undo}** | Undo |
| **{redo}** | Redo |

## Selection

| Key | Action |
|-----|--------|
| **Shift+Arrow** | Select text |
| **Alt+Shift+Arrow** | Block (rectangular) selection |
| **Ctrl+A** | Select all |
| **{copy}** | Copy selection |
| **{cut}** | Cut selection |
| **{paste}** | Paste |
| **ESC** | Clear selection |
| **Mouse drag** | Select text |
| **Alt+Mouse drag** | Block (rectangular) selection |
| **Click line #** | Select entire line |

## Search

| Key | Action |
|-----|--------|
| **{find}** | Open find (case-insensitive regex by default) |
| **{find_next}** | Find next occurrence |
| **{find_previous}** | Find previous occurrence |

## File Operations

| Key | Action |
|-----|--------|
| **{save}** | Save file |
| **{close}** | Close file (returns to file selector) |
| **{quit}** | Quit editor (double-tap within {double_tap_speed_ms}ms) |

**File Menu:**
- **New**: Create a new untitled file
- **Open...**: Browse and open files from directory tree
- **Save**: Save current file (prompts for name if untitled)
- **Close**: Close current file

**View Menu:**
- **Line Wrap**: Toggle line wrapping on/off
- **Rendered**: Toggle markdown rendered view (only available for `.md` files)
  - Shows the document rendered as formatted markdown (bold, italic, tables, etc.)
  - Rendered view is read-only — switch back to plain to edit
  - Search (`{find}`) and scrolling still work in rendered view

Untitled files show as "(unsaved)" in the header until you save them with a real filename.

## Other

| Key | Action |
|-----|--------|
| **{help}** | Show this help |
| **Mouse scroll** | Scroll viewport (Shift+Scroll for horizontal) |

---

**Note:** Keybindings can be customized in `~/.ue/settings.toml`

