# ue – Simple Terminal File Viewer
A terminal-based file editor with configurable key bindings that strives to be as convenient as modern UI-based editors. The name `u`e is an abbreviation for Uli's Editor (referring to the author's name, Uli Ruffler).

One of the main reasons for creating this editor was to explore how to work with Copilot. Thus, the code is heavily AI-based, and almost everything is generated.


## Features

- Line numbering
- Wrapping of long lines
- Cursor navigation with arrow keys, including common combinations of `Ctrl` and `Shift`
- Mouse cursor positioning, selection, and scrolling
- Scroll bar
- Extensible syntax highlighting
- Configurable key bindings via a `TOML` configuration file
- Persistent undo mechanism
- Persist scroll and cursor position
- Multi-instance usage
- File selector page (replacement for tabs)
- Find with highlighting while typing
- Cursor position and “go to” functionality
- Help pages


## Navigation (default)

- `Esc` leaves a mode (selection, find, go to, help) or toggles between the editor and the file selector page
- `F1` for help
- Arrow keys move the cursor one character or line
- `Shift+ARROW` selects text (line-wise)
- `Alt+Shift+ARROW` selects text in block mode (column-based, across multiple lines)
- `Ctrl+ARROW` moves the cursor one word or paragraph
- `Pos1` moves to the first non-blank character or the beginning of the line
- `End` moves to the end of the line
- `Ctrl+f` enters find mode (regex search)
- `Ctrl+g` enters go-to mode
- Double-tap `Esc` to immediately exit ue (this doesn't save the file, but you won't lose changes — just come back)
- `Ctrl+q` exits the editor
- `Ctrl+w` closes the file
- `Ctrl+s` saves the file

### Block Selection

Block selection allows you to select a rectangular region of text, useful for editing columns across multiple lines:

- Hold `Alt` while clicking and dragging with the mouse to create a block selection
- Use `Alt+Shift+ARROW` keys to extend a block selection with the keyboard
- When a block selection has zero width (same column), typing inserts characters on all selected lines simultaneously
- **Multi-line cursor**: Uses the normal vertical line cursor (not a block cursor)
- **Direction changes**: Block selection can change direction while selecting - move left/right or up/down freely
- Copy, cut, delete, and paste operations work with block selections
- Lines shorter than the selection range are partially selected or skipped
- Block selection works in both directions (left-to-right and right-to-left)

### Multi-Cursor Mode

Create multiple independent cursors to edit several lines at once:

- **Alt+Up/Down**: Add cursors above/below the current cursor position
- **Typing**: Characters are inserted at all cursor positions simultaneously
- **Backspace/Delete**: Removes characters at all cursor positions
- **Visual feedback**: Shows blinking block cursors - the character at each cursor position alternates between normal and inverted every 500ms
- **Exit**: Press **Esc** to clear all multi-cursors, or press any navigation key (arrows, Home, End, PageUp/PageDown)

**Note**: Alt+Mouse is used for block selection, not multi-cursor. Use Alt+Up/Down for multi-cursor mode.


## License
This project is licensed under the GNU General Public License v3.0 - see the LICENSE file for details.