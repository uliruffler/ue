# Find Mode Help

Press **{help}** or **ESC** to close this help.

## Find Mode - Search with Regular Expressions (Case-Insensitive by Default)

### Basic Usage

| Key | Action |
|-----|--------|
| **Type pattern** | Enter search pattern (supports regex) |
| **Enter** | Search forward and close find mode |
| **ESC** | Cancel and close find mode |
| **{find_next}** | Find next occurrence |
| **{find_previous}** | Find previous occurrence |

### Navigation in Find

| Key | Action |
|-----|--------|
| **Left** / **Right** | Move cursor in pattern |
| **Home** / **End** | Jump to start/end of pattern |
| **Backspace** | Delete character before cursor |
| **Up** / **Down** | Navigate search history |

### Search Behavior

- Searches are **case-INSENSITIVE** by default
- Pattern supports regex: `\d+` (digits), `\w+` (words), `.*` (any), etc.
- Live highlighting shows matches as you type
- Search wraps around file (shows warning before wrapping)
- If text is selected, search is scoped to selection only

### Case-Sensitive Search

To perform case-sensitive search, use regex flag:
- **Syntax:** `(?-i)pattern`
- **Example:** `(?-i)Hello` matches only 'Hello', not 'hello' or 'HELLO'

### Regex Examples

| Pattern | Matches |
|---------|---------|
| `hello` | hello, Hello, HELLO (case-insensitive) |
| `(?-i)hello` | hello only (case-sensitive) |
| `\d+` | one or more digits (123, 4567) |
| `\w+` | word characters (foo, bar_123) |
| `fo+` | f followed by one or more o (fo, foo, fooo) |
| `(cat\|dog)` | cat OR dog |
| `^start` | 'start' at beginning of line |
| `end$` | 'end' at end of line |

### Tips

- Search history is saved and can be accessed with **Up**/**Down** arrows
- History persists across sessions

---

**Note:** Keybindings can be customized in `~/.ue/settings.toml`

