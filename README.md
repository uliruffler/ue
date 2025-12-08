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
- `Ctrl+ARROW` moves the cursor one word or paragraph
- `Pos1` moves to the first non-blank character or the beginning of the line
- `End` moves to the end of the line
- `Ctrl+f` enters find mode (regex search)
- `Ctrl+g` enters go-to mode
- Double-tap `Esc` to immediately exit ue (this doesn’t save the file, but you won’t lose changes—just come back)
- `Ctrl+q` exits the editor
- `Ctrl+w` closes the file
- `Ctrl+s` saves the file


## License
This project is licensed under the GNU General Public License v3.0 - see the LICENSE file for details.