# Module Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                         main.rs                             │
│                    (Entry Point)                            │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                         ui.rs                               │
│                   (Orchestration Layer)                     │
│  • show() - Setup/teardown terminal                         │
│  • display_file_contents() - Iterate files                  │
│  • display_file_content() - Main event loop                 │
│                                                             │
│  Coordinates the interaction between:                       │
└──┬────────┬─────────┬──────────┬──────────┬─────────┬───────┘
   │        │         │          │          │         │
   │        │         │          │          │         │
   ▼        ▼         ▼          ▼          ▼         ▼
┌──────┐ ┌──────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
│editor│ │coord │ │render  │ │editing │ │event   │ │settings│
│state │ │inates│ │ing     │ │        │ │handlers│ │        │
└──────┘ └──────┘ └────────┘ └────────┘ └────────┘ └────────┘
   │        │         │          │          │          │
   │        │         │          │          │          │
   ▼        ▼         ▼          ▼          ▼          ▼


┌──────────────────────────────────────────────────────────────┐
│                    editor_state.rs (77 lines)                │
├──────────────────────────────────────────────────────────────┤
│ FileViewerState struct & methods                             │
│ • top_line, cursor_line, cursor_col                          │
│ • selection_start, selection_end                             │
│ • needs_redraw, modified, mouse_dragging                     │
│ • new(), current_position(), absolute_line()                 │
│ • has_selection(), start/update/clear_selection()            │
└──────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────────────────────────────┐
│                   coordinates.rs (143 lines)                 │
├──────────────────────────────────────────────────────────────┤
│ Position calculations & transformations                      │
│ • line_number_width() - Calculate gutter width               │
│ • calculate_wrapped_lines_for_line() - Line wrapping         │
│ • calculate_cursor_visual_line() - Map to screen             │
│ • visual_to_logical_position() - Mouse → file position       │
│ • adjust_view_for_resize() - Terminal resize handling        │
│ + 5 tests for resize behavior                                │
└──────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────────────────────────────┐
│                    rendering.rs (291 lines)                  │
├──────────────────────────────────────────────────────────────┤
│ Screen rendering logic                                       │
│ • render_screen() - Main orchestrator                        │
│ • render_header() - File info + line number block            │
│ • render_footer() - Position info                            │
│ • render_visible_lines() - Content area                      │
│ • render_line() - Single line with wrapping                  │
│ • render_line_segment_with_selection() - Highlighting        │
│ • position_cursor() - Move cursor to correct position        │
└──────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────────────────────────────┐
│                     editing.rs (596 lines)                   │
├──────────────────────────────────────────────────────────────┤
│ Text editing & clipboard operations                          │
│ • GLOBAL_CLIPBOARD - Persistent clipboard instance           │
│ • handle_copy/paste/cut() - High-level operations            │
│ • save_file() - Write to disk                                │
│ • insert_char/delete_backward/delete_forward() - Primitives  │
│ • split_line/insert_tab() - Line operations                  │
│ • remove_selection() - Selection deletion                    │
│ • extract_selection() - Get selected text                    │
│ • apply_undo/apply_redo() - History operations               │
│ • handle_editing_keys() - Main dispatcher                    │
└──────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────────────────────────────┐
│                  event_handlers.rs (219 lines)               │
├──────────────────────────────────────────────────────────────┤
│ Event processing & navigation                                │
│ • handle_key_event() - Keyboard input processing             │
│ • handle_mouse_event() - Mouse events (NEW: scrolling!)      │
│   - Click/drag for selection                                 │
│   - ScrollUp/ScrollDown ← NEW FEATURE                        │
│ • handle_navigation() - Cursor movement                      │
│ • is_exit_command/is_navigation_key() - Key detection        │
│ • update_selection_state/update_redraw_flags() - State sync  │
└──────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────────────────────────────┐
│                      undo.rs (335 lines)                     │
├──────────────────────────────────────────────────────────────┤
│ Undo/redo history management (unchanged)                     │
│ • Edit enum - All edit types                                 │
│ • UndoHistory struct - History stack                         │
│ • save/load() - Persistence                                  │
│ • update_state/update_cursor() - State tracking              │
│ + 13 tests                                                   │
└──────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────────────────────────────┐
│                    settings.rs (188 lines)                   │
├──────────────────────────────────────────────────────────────┤
│ Configuration & keybindings                                  │
│ • Settings struct - Configuration                            │
│ • KeyBindings - Key mapping                                  │
│ • load() - Read from ~/.ue/settings.toml                     │
│ • *_matches() - Check keybinding match                       │
│ • enable_syntax_highlighting flag                            │
│ + 7 tests                                                    │
└──────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────────────────────────────┐
│                     syntax.rs (~310 lines)                   │
├──────────────────────────────────────────────────────────────┤
│ Syntax definition parsing & color conversion                 │
│ • SyntaxDefinition - Keywords, matches, colors               │
│ • ColorSpec - ANSI colors, bold/italic attributes            │
│ • SyntaxMatch - Regex-based patterns                         │
│ • get_syntax_for_extension() - Load from ~/.ue/syntax/       │
│ • parse_vim_syntax() - Parse simplified Vim syntax           │
│ • ColorSpec::apply_to_stdout() - Apply colors to terminal    │
│ • SYNTAX_CACHE - Cache parsed definitions                    │
│ + 4 tests                                                    │
└──────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────────────────────────────┐
│                   highlighter.rs (~150 lines)                │
├──────────────────────────────────────────────────────────────┤
│ Line-level syntax highlighting application                   │
│ • StyledSpan - Position + color for a text span              │
│ • highlight_line() - Apply syntax to a line of text          │
│   - First pass: regex pattern matches                        │
│   - Second pass: keyword matches (word boundaries)           │
│   - Regex spans take precedence over keywords                │
│ • get_file_extension() - Extract extension from filename     │
│ + 3 tests                                                    │
└──────────────────────────────────────────────────────────────┘


Data Flow Example (Mouse Scroll):
─────────────────────────────────

User scrolls mouse wheel ↓
         │
         ▼
ui.rs: Event::Mouse(mouse_event)
         │
         ▼
event_handlers::handle_mouse_event()
  Detects: MouseEventKind::ScrollDown
  Updates: state.top_line += 3
  Sets: state.needs_redraw = true
         │
         ▼
ui.rs: if state.needs_redraw
         │
         ▼
rendering::render_screen()
  Uses coordinates::calculate_*()
  Renders visible lines
         │
         ▼
Screen updated! ✨
```

Data Flow Example (Syntax Highlighting):
─────────────────────────────────────────

rendering::render_line_segment()
  Settings check: state.settings.enable_syntax_highlighting?
         │
         ▼
rendering::get_highlight_spans()
  Extracts file extension
         │
         ▼
syntax::get_syntax_for_extension()
  1. Check SYNTAX_CACHE
  2. If miss: load ~/.ue/syntax/{ext}.vim
  3. parse_vim_syntax() - Parse keywords, matches, colors
  4. Cache result
  Returns: SyntaxDefinition
         │
         ▼
highlighter::highlight_line()
  1. Apply regex patterns → StyledSpan[]
  2. Apply keywords (word boundaries) → candidates
  3. Filter overlaps (regex wins)
  4. Sort by position
  Returns: Vec<StyledSpan>
         │
         ▼
rendering::render_with_highlighting()
  For each character:
    - Find matching span
    - Apply ColorSpec::apply_to_stdout()
    - Write character
    - Reset color
         │
         ▼
Highlighted text rendered! 🎨
```

## Key Design Principles

1. **Single Responsibility**: Each module has one clear job
2. **No Public APIs**: All cross-module items use `pub(crate)`
3. **Clear Dependencies**: ui.rs orchestrates, modules don't call each other
4. **Testable**: Logic isolated for easy unit testing
5. **Extensible**: New features fit naturally into existing structure

