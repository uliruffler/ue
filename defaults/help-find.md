# Find Mode Help

Press **{help}** or **ESC** to close this help.

## Find Mode - Search with Regular Expressions or Wildcards (Case-Insensitive by Default)

### Search Modes

**Toggle between two search modes using the ⇄ button:**

- **⇄R (Regex Mode)**: Full regex pattern matching (default)
  - Click the `[⇄R]` button or press **{toggle_find_mode}** to switch modes
  - Footer shows: `Find [⇄R]: `
  - Supports advanced patterns: `\d+`, `\w+`, `.*`, `^start`, `end$`, etc.
  
- **⇄W (Wildcard Mode)**: Simple wildcard matching (like shell patterns)
  - Click the `[⇄W]` button or press **{toggle_find_mode}** to switch modes
  - Footer shows: `Find [⇄W]: `
  - `*` matches any number of characters (including zero)
  - `?` matches exactly one character
  - Special characters (`.`, `[`, `]`, etc.) are treated as literals
  - Example: `test*.txt` matches `test.txt`, `test123.txt`, `testfile.txt`

### Basic Usage

| Key | Action |
|-----|--------|
| **Type pattern** | Enter search pattern (regex or wildcard) |
| **{toggle_find_mode}** or **Click mode** | Toggle between regex and wildcard mode |
| **Enter** | Search forward and close find mode |
| **ESC** | Cancel and close find mode |
| **{find_next}** | Find next occurrence |
| **{find_previous}** | Find previous occurrence |
| **{replace}** | Enter replace mode (after search is active) |

### Replace Mode

After performing a search, press **{replace}** to enter replace mode:

| Key | Action |
|-----|--------|
| **Type text** | Enter replacement text |
| **Enter** | Show replace buttons (doesn't execute) |
| **ESC** | Exit replace mode (return to normal edit mode) |
| **{replace_current}** | Replace current match and jump to next |
| **{replace_all}** | Replace all matches and exit replace mode |
| **Click button** | Click `[replace occurrence]` or `[replace all]` |

**Replace Workflow:**
1. Press **{find}** and search for pattern
2. Press **{replace}** to enter replace mode
3. Type replacement text
4. Press **Enter** to see buttons (or use keyboard shortcuts)
5. Use **{replace_current}** to replace one at a time, or **{replace_all}** to replace all at once
6. Press **ESC** to exit replace mode

**Note:** Replace respects search scope - if you searched within a selection, only that selection will be affected.

### Navigation in Find

| Key | Action |
|-----|--------|
| **Left** / **Right** | Move cursor in pattern |
| **Home** / **End** | Jump to start/end of pattern |
| **Backspace** | Delete character before cursor |
| **Up** / **Down** | Navigate search history |

### Search Behavior

- Searches are **case-INSENSITIVE** by default in both modes
- **Regex mode**: Full pattern support (`\d+`, `\w+`, `.*`, etc.)
- **Wildcard mode**: Simple patterns (`*`, `?`, literal characters)
- Live highlighting shows matches as you type
- Search wraps around file automatically (no confirmation needed)
- If text is selected, search is scoped to selection only
- Mode toggle button (**[⇄R]** or **[⇄W]**) shows active mode
- **Hit counter** shows `(X/Y) ↑↓  line:col` format
  - Always visible when search is active (even with 0 matches)
  - Format: `(current/total) ↑↓  line:col`
  - Example: `(2/5) ↑↓  12:5` means at hit 2 of 5, cursor at line 12, column 5
  - Shows `(0) ↑↓  12:5` when no matches found
  - Shows `(-/5) ↑↓  12:5` when cursor is not on any match
  - Click **↑** or **↓** arrows to navigate between matches
  - Position always visible (never hidden)

### Search Workflow

1. **Press {find}** to enter find mode
2. **Type pattern** - see hit count update in real-time: `(5) ↑↓  12:5`
3. **Press Enter** - exits find mode, highlights remain, cursor stays put
4. **Press {find_next}** or click **↓** - jump to first/next match
5. **Press {find_previous}** or click **↑** - jump to previous match
6. Wraps automatically (no confirmation)

### Exiting Search

- **ESC while typing**: Exits find mode, restores previous highlights
- **ESC after Enter**: Clears search highlights (first press)
- **ESC ESC (double-tap)**: Exits editor immediately

### Filter Mode

Filter mode shows only matching lines (and optionally context lines around them), hiding all other lines:

1. **Activate filter mode**: Press **{find}** again while viewing search results (or press **{find}** twice in find mode)
2. **Adjust context lines**: Use spinners in footer
   - **Mouse**: Click **▲** or **▼** arrows next to "Before" and "After" numbers to increase/decrease by 1
   - **Default values**: Set in `~/.ue/settings.toml` with `filter_context_before` and `filter_context_after` (0-9)
3. **Navigate**: Use arrow keys to move between visible lines (matching + context)
4. **Exit filter mode**: Press **{find}** again to toggle back to normal view

**Filter Mode Features:**
- Shows `Filter: ` label in footer with context spinners
- Footer displays: `Filter: Before:X▲▼ After:Y▲▼` (X and Y are number of context lines)
- Context lines range from 0-9 with mouse control
- Navigation automatically jumps between visible lines (including context)
- All editing works normally on visible lines

### Case-Sensitive Search

To perform case-sensitive search, use regex flag:
- **Syntax:** `(?-i)pattern`
- **Example:** `(?-i)Hello` matches only 'Hello', not 'hello' or 'HELLO'

### Wildcard Examples

**Wildcard mode treats special regex characters as literals:**

| Pattern | Matches | Doesn't Match |
|---------|---------|---------------|
| `*.txt` | `file.txt`, `test.txt`, `abc123.txt` | `file.doc`, `txt` |
| `test?` | `test1`, `testa`, `test!` | `test`, `test12` |
| `foo*bar` | `foobar`, `foo123bar`, `foo_test_bar` | `fobar`, `foobaz` |
| `test.txt` | `test.txt` (literal dot) | `testXtxt` |
| `[abc]` | `[abc]` (literal brackets) | `a`, `b`, `c` |
| `file*.?` | `file.x`, `file123.a` | `file.`, `file.ab` |

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
| `test.txt` | matches `testXtxt` (. = any char) |
| `[abc]` | matches `a`, `b`, or `c` |

### Tips

- Search history is saved and can be accessed with **Up**/**Down** arrows
- History persists across sessions

---

**Note:** Keybindings can be customized in `~/.ue/settings.toml`

