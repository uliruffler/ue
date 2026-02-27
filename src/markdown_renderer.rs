//! Pluggable Markdown rendering for the editor.
//!
//! The central abstraction is the [`MarkdownRenderer`] trait.  Each implementation
//! converts a Markdown string into a `Vec<String>` of display lines that may contain
//! ANSI escape codes.
//!
//! Available renderers:
//! - [`PulldownRenderer`] — custom ANSI renderer built on `pulldown-cmark` (default)
//! - [`TermimadRenderer`] — legacy renderer based on the `termimad` crate
//!
//! A renderer is selected at runtime via [`MarkdownRenderer::from_name`], making it
//! trivial to add new output formats later (e.g. plain-text, HTML, …).

// ─── Trait ───────────────────────────────────────────────────────────────────

/// Convert Markdown source into terminal display lines.
///
/// Implementations may produce ANSI-escaped output or plain text.
/// `term_width` is the maximum column width to wrap at.
pub(crate) trait MarkdownRenderer: Send + Sync {
    fn render(&self, markdown: &str, term_width: usize) -> Vec<String>;
}

// ─── Registry ────────────────────────────────────────────────────────────────

/// Return a renderer by name.  Unknown names fall back to `"pulldown"`.
///
/// Recognised names (case-insensitive):
/// - `"pulldown"` — [`PulldownRenderer`]
/// - `"termimad"` — [`TermimadRenderer`]
#[allow(dead_code)] // Extension point: used when adding new renderer types at runtime
pub(crate) fn renderer_from_name(name: &str) -> Box<dyn MarkdownRenderer> {
    match name.to_ascii_lowercase().as_str() {
        "termimad" => Box::new(TermimadRenderer),
        _ => Box::new(PulldownRenderer),
    }
}

/// Return the default renderer.
pub(crate) fn default_renderer() -> Box<dyn MarkdownRenderer> {
    Box::new(PulldownRenderer)
}

// ─── pulldown-cmark renderer ─────────────────────────────────────────────────

/// Renderer built on `pulldown-cmark`.  Produces clean, coloured ANSI output
/// without depending on `termimad`.
pub(crate) struct PulldownRenderer;

impl MarkdownRenderer for PulldownRenderer {
    fn render(&self, markdown: &str, term_width: usize) -> Vec<String> {
        render_pulldown(markdown, term_width)
    }
}

// ANSI helpers
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ITALIC: &str = "\x1b[3m";
const BOLD_ITALIC: &str = "\x1b[1;3m";
const UNDERLINE: &str = "\x1b[4m";
const DIM: &str = "\x1b[2m";

// Heading colours (bold + colour)
const H1: &str = "\x1b[1;36m"; // bold cyan
const H2: &str = "\x1b[1;33m"; // bold yellow
const H3: &str = "\x1b[1;32m"; // bold green
const H4: &str = "\x1b[1;35m"; // bold magenta

// Inline code / code block
const CODE_FG: &str = "\x1b[38;5;215m"; // orange-ish
const CODE_BG: &str = "\x1b[48;5;236m"; // dark grey background

// Blockquote
const QUOTE_FG: &str = "\x1b[2;37m"; // dim white

// Rule
const RULE_FG: &str = "\x1b[2;37m"; // dim white

// List bullet / ordered number
const BULLET_FG: &str = "\x1b[1;36m"; // bold cyan
const NUM_FG: &str = "\x1b[1;33m"; // bold yellow

/// Core pulldown-cmark rendering logic.
fn render_pulldown(markdown: &str, term_width: usize) -> Vec<String> {
    use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

    let effective_width = term_width.max(20);

    let options = Options::all();
    let parser = Parser::new_ext(markdown, options);

    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();

    // Nesting state
    let mut heading_level: Option<(u8, &'static str)> = None; // (level, indent_prefix)
    let mut in_code_block = false;
    #[allow(unused_assignments)]
    let mut code_block_lang = String::new();
    let mut in_blockquote = false;
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut ordered_counters: Vec<u64> = Vec::new();
    // Plain-text prefix prepended to wrapped continuation lines inside a list item.
    // Set when Start(Item) fires; the width matches the bullet/number marker so that
    // wrapped text aligns with the first line of the item.
    let mut list_continuation_indent = String::new();

    // Inline style stack
    let mut bold_depth: u32 = 0;
    let mut italic_depth: u32 = 0;
    let mut strikethrough_depth: u32 = 0;

    // Table accumulation
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut _in_table_header = false;

    macro_rules! push_line {
        () => {{
            lines.push(std::mem::take(&mut current_line));
        }};
    }

    let current_style = |bold: u32, italic: u32, _strike: u32| -> &'static str {
        match (bold > 0, italic > 0) {
            (true, true) => BOLD_ITALIC,
            (true, false) => BOLD,
            (false, true) => ITALIC,
            _ => "",
        }
    };

    let wrap_text =
        |text: &str, prefix: &str, effective_width: usize, lines: &mut Vec<String>, current_line: &mut String| {
            // Split text into words and wrap
            for word in text.split_inclusive(char::is_whitespace) {
                let word_visual = visual_len(word);
                let line_visual = visual_len(current_line);
                if line_visual > 0 && line_visual + word_visual > effective_width {
                    lines.push(std::mem::take(current_line));
                    current_line.push_str(prefix);
                }
                current_line.push_str(word);
            }
        };

    for event in parser {
        match event {
            // ── Block opens ──────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                if !current_line.is_empty() {
                    push_line!();
                }
                lines.push(String::new()); // blank before heading
                let lvl: u8 = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    _ => 4,
                };
                let color = match lvl {
                    1 => H1,
                    2 => H2,
                    3 => H3,
                    _ => H4,
                };
                // Visual indent prefix — no raw "#" characters
                let indent: &'static str = match lvl {
                    1 => "  ",
                    2 => "  ",
                    3 => "  ▸ ",
                    _ => "  › ",
                };
                heading_level = Some((lvl, indent));
                current_line.push_str(color);
                current_line.push_str(indent);
            }

            Event::End(TagEnd::Heading(_)) => {
                current_line.push_str(RESET);
                push_line!();
                // Underline for H1/H2, indented to match the heading text
                if let Some((lvl, indent)) = heading_level.take() {
                    let line_above = lines.last().cloned().unwrap_or_default();
                    // Total visual length of the heading line (indent + text)
                    let total_vis = visual_len(&line_above).min(effective_width);
                    // The underline should start at the same column as the text,
                    // so its width = total_vis - indent_width
                    let indent_w = visual_len(indent);
                    let rule_w = total_vis.saturating_sub(indent_w);
                    if lvl == 1 {
                        lines.push(format!("{}{}{}{}", H1, indent, "═".repeat(rule_w), RESET));
                    } else if lvl == 2 {
                        lines.push(format!("{}{}{}{}", H2, indent, "─".repeat(rule_w), RESET));
                    }
                }
                lines.push(String::new()); // blank after heading
            }

            Event::Start(Tag::Paragraph) => {
                if !current_line.is_empty() {
                    push_line!();
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if !current_line.is_empty() {
                    push_line!();
                }
                lines.push(String::new());
            }

            Event::Start(Tag::BlockQuote(_)) => {
                in_blockquote = true;
                if !current_line.is_empty() {
                    push_line!();
                }
                current_line.push_str(&format!("{}▌ {}", QUOTE_FG, RESET));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_blockquote = false;
                if !current_line.is_empty() {
                    push_line!();
                }
                lines.push(String::new());
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                use pulldown_cmark::CodeBlockKind;
                in_code_block = true;
                code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                if !current_line.is_empty() {
                    push_line!();
                }
                // Total box width = effective_width.
                // Top:    ╔══ lang ══...══╗   fixed overhead = "╔══ " (4) + " " (1) + "══╗" (3) = 8 + lang
                //         ╔══════════════╗    fixed overhead = "╔══" (3) + "══╗" (3) = 6
                let label = if code_block_lang.is_empty() {
                    let fill = effective_width.saturating_sub(6); // ╔══ + fill + ══╗
                    format!("{}{}╔══{}══╗{}", CODE_BG, CODE_FG, "═".repeat(fill), RESET)
                } else {
                    let lang = &code_block_lang;
                    let overhead = 8 + lang.len(); // ╔══ (3) + space + lang + space + ══╗ (3) → "╔══ lang ══╗" = 3+1+lang+1+3=8+lang, but we want extra fill
                    let fill = effective_width.saturating_sub(overhead);
                    format!("{}{}╔══ {} {}══╗{}", CODE_BG, CODE_FG, lang, "═".repeat(fill), RESET)
                };
                lines.push(label);
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                if !current_line.is_empty() {
                    push_line!();
                }
                // Bottom: ╚══════════════╝  — same width as top without lang
                let fill = effective_width.saturating_sub(6); // ╚══ + fill + ══╝
                lines.push(format!(
                    "{}{}╚══{}══╝{}",
                    CODE_BG, CODE_FG, "═".repeat(fill), RESET
                ));
                lines.push(String::new());
            }

            Event::Start(Tag::List(start)) => {
                list_stack.push(start);
                ordered_counters.push(start.unwrap_or(1));
                if !current_line.is_empty() {
                    push_line!();
                }
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
                ordered_counters.pop();
                if !current_line.is_empty() {
                    push_line!();
                }
                lines.push(String::new());
            }

            Event::Start(Tag::Item) => {
                let depth = list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                if let Some(Some(_)) = list_stack.last() {
                    // ordered — marker is e.g. "1. " (number + ". ")
                    let n = ordered_counters.last_mut().unwrap();
                    let marker_text = format!("{}. ", n); // plain text for width measurement
                    let marker = format!("{}{}{}{}. {}", indent, NUM_FG, BOLD, n, RESET);
                    *n += 1;
                    current_line.push_str(&marker);
                    // continuation indent = base indent + width of "N. "
                    list_continuation_indent =
                        format!("{}{}", indent, " ".repeat(visual_len(&marker_text)));
                } else {
                    // bullet — marker is "• " (1 char + space = 2 visible cols)
                    let bullet = match depth % 3 {
                        0 => "•",
                        1 => "◦",
                        _ => "▪",
                    };
                    current_line.push_str(&format!("{}{}{} {}", indent, BULLET_FG, bullet, RESET));
                    // continuation indent = base indent + "  " (bullet + space = 2 cols)
                    list_continuation_indent = format!("{}  ", indent);
                }
            }
            Event::End(TagEnd::Item) => {
                if !current_line.is_empty() {
                    push_line!();
                }
            }

            // ── Tables ────────────────────────────────────────────────────
            Event::Start(Tag::Table(_)) => {
                in_table = true;
                table_rows.clear();
                if !current_line.is_empty() {
                    push_line!();
                }
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                if !current_row.is_empty() {
                    table_rows.push(std::mem::take(&mut current_row));
                }
                let rendered = render_table(&table_rows, effective_width);
                lines.extend(rendered);
                lines.push(String::new());
            }
            Event::Start(Tag::TableHead) => { _in_table_header = true; }
            Event::End(TagEnd::TableHead) => {
                _in_table_header = false;
                if !current_cell.is_empty() {
                    current_row.push(std::mem::take(&mut current_cell));
                }
                table_rows.push(std::mem::take(&mut current_row));
                table_rows.push(vec!["__HEADER_SEP__".to_string()]);
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                if !current_cell.is_empty() {
                    current_row.push(std::mem::take(&mut current_cell));
                }
                if !current_row.is_empty() {
                    table_rows.push(std::mem::take(&mut current_row));
                }
            }
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => {
                current_row.push(std::mem::take(&mut current_cell));
            }

            // ── Inline formatting ─────────────────────────────────────────
            Event::Start(Tag::Strong) => {
                bold_depth += 1;
                let style = current_style(bold_depth, italic_depth, strikethrough_depth);
                if in_table { current_cell.push_str(style); } else { current_line.push_str(style); }
            }
            Event::End(TagEnd::Strong) => {
                bold_depth = bold_depth.saturating_sub(1);
                let style = current_style(bold_depth, italic_depth, strikethrough_depth);
                if in_table {
                    current_cell.push_str(RESET);
                    if !style.is_empty() { current_cell.push_str(style); }
                } else {
                    current_line.push_str(RESET);
                    if !style.is_empty() { current_line.push_str(style); }
                }
            }

            Event::Start(Tag::Emphasis) => {
                italic_depth += 1;
                let style = current_style(bold_depth, italic_depth, strikethrough_depth);
                if in_table { current_cell.push_str(style); } else { current_line.push_str(style); }
            }
            Event::End(TagEnd::Emphasis) => {
                italic_depth = italic_depth.saturating_sub(1);
                let style = current_style(bold_depth, italic_depth, strikethrough_depth);
                if in_table {
                    current_cell.push_str(RESET);
                    if !style.is_empty() { current_cell.push_str(style); }
                } else {
                    current_line.push_str(RESET);
                    if !style.is_empty() { current_line.push_str(style); }
                }
            }

            Event::Start(Tag::Strikethrough) => {
                strikethrough_depth += 1;
                if in_table { current_cell.push_str(DIM); } else { current_line.push_str(DIM); }
            }
            Event::End(TagEnd::Strikethrough) => {
                strikethrough_depth = strikethrough_depth.saturating_sub(1);
                if in_table { current_cell.push_str(RESET); } else { current_line.push_str(RESET); }
            }

            Event::Start(Tag::Link { dest_url, title, .. }) => {
                let label = if title.is_empty() { dest_url.to_string() } else { title.to_string() };
                let link_str = format!("{}{}{}{}", UNDERLINE, DIM, label, RESET);
                if in_table { current_cell.push_str(&link_str); } else { current_line.push_str(&link_str); }
            }
            Event::End(TagEnd::Link) => {}
            Event::Start(Tag::Image { .. }) => {}
            Event::End(TagEnd::Image) => {}

            // ── Leaf events ───────────────────────────────────────────────
            Event::Text(text) => {
                if in_code_block {
                    // Box inner width: effective_width - 4  ("║ " prefix + " ║" suffix)
                    let inner_w = effective_width.saturating_sub(4);
                    for code_line in text.lines() {
                        if !current_line.is_empty() {
                            push_line!();
                        }
                        // Hard-wrap long lines at inner_w
                        let mut remaining = code_line.to_string();
                        loop {
                            let (chunk, rest) = split_at_visual_width(&remaining, inner_w);
                            let chunk_vis = visual_len(&chunk);
                            let pad = inner_w.saturating_sub(chunk_vis);
                            // ║ + space + content + padding + space + ║
                            current_line.push_str(&format!(
                                "{}{}║ {}{}{} ║{}",
                                CODE_BG, CODE_FG,
                                chunk,
                                " ".repeat(pad),
                                CODE_FG,  // re-assert colour after chunk (which may reset)
                                RESET
                            ));
                            push_line!();
                            if rest.is_empty() { break; }
                            remaining = rest;
                        }
                    }
                } else if in_table {
                    current_cell.push_str(&text);
                } else {
                    let prefix = if in_blockquote {
                        format!("{}▌ {}", QUOTE_FG, RESET)
                    } else if !list_stack.is_empty() {
                        list_continuation_indent.clone()
                    } else {
                        String::new()
                    };
                    wrap_text(&text, &prefix, effective_width, &mut lines, &mut current_line);
                }
            }

            Event::Code(code) => {
                let styled = format!("{}{} {}{}", CODE_BG, CODE_FG, code, RESET);
                if in_table { current_cell.push_str(&styled); } else { current_line.push_str(&styled); }
            }

            Event::SoftBreak => {
                if !in_table && !in_code_block {
                    current_line.push(' ');
                }
            }
            Event::HardBreak => {
                if !in_table {
                    push_line!();
                    if in_blockquote {
                        current_line.push_str(&format!("{}▌ {}", QUOTE_FG, RESET));
                    }
                }
            }

            Event::Rule => {
                if !current_line.is_empty() { push_line!(); }
                lines.push(format!("{}{}{}", RULE_FG, "─".repeat(effective_width), RESET));
                lines.push(String::new());
            }

            Event::Html(html) => {
                for html_line in html.lines() {
                    if !current_line.is_empty() { push_line!(); }
                    current_line.push_str(&format!("{}{}{}", DIM, html_line, RESET));
                }
            }
            Event::InlineHtml(html) => {
                current_line.push_str(&format!("{}{}{}", DIM, html, RESET));
            }
            Event::FootnoteReference(label) => {
                current_line.push_str(&format!("[{}]", label));
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked {
                    format!("{}[✓]{} ", BOLD, RESET)
                } else {
                    format!("{}[ ]{} ", DIM, RESET)
                };
                current_line.push_str(&marker);
            }

            // Ignore remaining tag variants we don't handle yet
            _ => {}
        }
    }

    if !current_line.is_empty() {
        push_line!();
    }

    lines
}

// ─── Table rendering helper ───────────────────────────────────────────────────

/// Render a collected table into ANSI display lines with word-wrapping per cell.
///
/// A sentinel row `["__HEADER_SEP__"]` separates the header rows from the body.
/// Each cell in `rows` may contain ANSI escape sequences; visual width is measured
/// by stripping them before padding calculations.
///
/// When a cell's content exceeds its column width the text is word-wrapped across
/// multiple lines.  All cells in the same logical row are padded to the same
/// visual height so the border characters always line up.
fn render_table(rows: &[Vec<String>], max_width: usize) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }

    // ── Split into header / body ───────────────────────────────────────────────
    let mut header: Vec<Vec<String>> = Vec::new();
    let mut body: Vec<Vec<String>> = Vec::new();
    let mut past_header = false;
    let mut col_count = 0;

    for row in rows {
        if row.len() == 1 && row[0] == "__HEADER_SEP__" {
            past_header = true;
            continue;
        }
        col_count = col_count.max(row.len());
        if past_header { body.push(row.clone()); } else { header.push(row.clone()); }
    }
    if col_count == 0 {
        return Vec::new();
    }

    // ── Measure natural column widths ──────────────────────────────────────────
    let mut col_widths = vec![1usize; col_count];
    for row in header.iter().chain(body.iter()) {
        for (i, cell) in row.iter().enumerate() {
            if i < col_count {
                let w = visual_len(cell);
                if w > col_widths[i] { col_widths[i] = w; }
            }
        }
    }

    // ── Constrain to terminal width ────────────────────────────────────────────
    // Layout: │ sp content sp │ sp content sp │ …
    // Total fixed cost = (col_count + 1) borders + col_count * 2 padding spaces
    let padding = 2usize; // one space on each side of every cell
    let fixed_cost = col_count + 1 + col_count * padding;
    let avail = max_width.saturating_sub(fixed_cost);
    if avail > 0 {
        let total_natural: usize = col_widths.iter().sum();
        if total_natural > avail {
            // Shrink each column proportionally; minimum 4 chars so wrapping is viable
            for w in col_widths.iter_mut() {
                *w = ((*w as f64 / total_natural as f64) * avail as f64).max(4.0) as usize;
            }
        }
    }

    // ── ANSI chrome ───────────────────────────────────────────────────────────
    const BORDER: &str = "\x1b[2;37m"; // dim white — borders only

    let hbar = |w: usize, ch: &str| ch.repeat(w + padding);

    let mut out = Vec::new();

    // ── Top border ─────────────────────────────────────────────────────────────
    let top: String = (0..col_count).map(|i| hbar(col_widths[i], "─")).collect::<Vec<_>>().join("┬");
    out.push(format!("{}┌{}┐{}", BORDER, top, RESET));

    // ── Render header rows ─────────────────────────────────────────────────────
    for row in &header {
        let wrapped = wrap_row(row, &col_widths, col_count);
        emit_row_lines(&wrapped, &col_widths, col_count, true, &mut out, BORDER);
    }

    // ── Header / body separator ────────────────────────────────────────────────
    let sep: String = (0..col_count).map(|i| hbar(col_widths[i], "═")).collect::<Vec<_>>().join("╪");
    out.push(format!("{}╞{}╡{}", BORDER, sep, RESET));

    // ── Render body rows ───────────────────────────────────────────────────────
    for row in &body {
        let wrapped = wrap_row(row, &col_widths, col_count);
        emit_row_lines(&wrapped, &col_widths, col_count, false, &mut out, BORDER);
    }

    // ── Bottom border ──────────────────────────────────────────────────────────
    let bot: String = (0..col_count).map(|i| hbar(col_widths[i], "─")).collect::<Vec<_>>().join("┴");
    out.push(format!("{}└{}┘{}", BORDER, bot, RESET));

    out
}

/// Word-wrap one cell's text (may contain ANSI sequences) into lines that fit
/// within `col_width` visible columns.
///
/// Words are split on whitespace.  A single word longer than `col_width` is
/// hard-wrapped at the column boundary; ANSI escape sequences are never split
/// and any open style is closed/re-opened across the break.
fn wrap_cell(text: &str, col_width: usize) -> Vec<String> {
    if col_width == 0 {
        return vec![String::new()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w = 0usize;

    for word in text.split_whitespace() {
        let word_w = visual_len(word);
        if current_w == 0 {
            // First word on this line — hard-break if it's wider than the column
            if word_w <= col_width {
                current.push_str(word);
                current_w = word_w;
            } else {
                // Hard-break the oversized word across multiple lines
                let mut remaining = word.to_string();
                while !remaining.is_empty() {
                    let (chunk, rest) = split_at_visual_width(&remaining, col_width);
                    if !current.is_empty() {
                        lines.push(std::mem::take(&mut current));
                        current_w = 0;
                    }
                    if rest.is_empty() {
                        current_w = visual_len(&chunk);
                        current = chunk;
                    } else {
                        lines.push(chunk);
                    }
                    remaining = rest;
                }
            }
        } else {
            // Subsequent word — fits on current line with a space?
            if current_w + 1 + word_w <= col_width {
                current.push(' ');
                current.push_str(word);
                current_w += 1 + word_w;
            } else {
                // Wrap: finish current line, start fresh
                lines.push(std::mem::take(&mut current));
                current_w = 0;
                if word_w <= col_width {
                    current.push_str(word);
                    current_w = word_w;
                } else {
                    let mut remaining = word.to_string();
                    while !remaining.is_empty() {
                        let (chunk, rest) = split_at_visual_width(&remaining, col_width);
                        if rest.is_empty() {
                            current_w = visual_len(&chunk);
                            current = chunk;
                        } else {
                            lines.push(chunk);
                        }
                        remaining = rest;
                    }
                }
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Split a string (which may contain ANSI escape sequences) at the last
/// character boundary that keeps the visible width within `max_visual` columns.
///
/// Returns `(head, tail)` as owned `String`s:
/// - `head` contains everything up to (but not including) the split point, plus
///   a `RESET` if any ANSI style was open at the split so the terminal is clean.
/// - `tail` re-opens any ANSI style that was active at the split point so the
///   text continues with the correct appearance.
///
/// ANSI sequences are never split mid-sequence.
fn split_at_visual_width(s: &str, max_visual: usize) -> (String, String) {
    // We walk the string tracking:
    //   - visible column width consumed so far
    //   - byte position of the last safe split point
    //   - the most-recently-seen complete ANSI escape sequence (for re-opening)
    //   - whether any ANSI sequence was active when we hit the split
    let mut width = 0usize;
    let mut split_byte = 0usize;        // byte index of the split point
    let mut split_width_reached = false;
    let mut last_escape = String::new(); // last complete escape seq seen before split
    let mut any_escape_before_split = false;

    let bytes = s.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'\x1b' {
            // Consume the entire escape sequence without counting it as width.
            let seq_start = i;
            i += 1; // skip ESC
            // CSI sequence: ESC '[' ... letter
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() && !bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if i < bytes.len() { i += 1; } // include the terminating letter
            }
            let seq = &s[seq_start..i];
            // Track escape state relative to the split point
            if !split_width_reached {
                last_escape = seq.to_string();
                // '\x1b[0m' (and variants) resets all styles
                if seq == "\x1b[0m" || seq == "\x1b[m" {
                    any_escape_before_split = false;
                    last_escape.clear();
                } else {
                    any_escape_before_split = true;
                }
            }
        } else {
            // Regular (possibly multi-byte) character
            let ch_start = i;
            // Decode one UTF-8 char
            let ch = s[i..].chars().next().unwrap_or('\0');
            let char_bytes = ch.len_utf8();
            use unicode_width::UnicodeWidthChar;
            let cw = ch.width().unwrap_or(1);

            if width + cw > max_visual {
                // This character would exceed the limit — split here
                split_byte = ch_start;
                split_width_reached = true;
                break;
            }
            width += cw;
            i += char_bytes;
            // Update split point after each visible character
            if !split_width_reached {
                split_byte = i;
            }
        }
    }

    if split_width_reached {
        // head = s[..split_byte] + RESET (if any style was open)
        // tail = re-open last style (if any) + s[split_byte..]
        let mut head = s[..split_byte].to_string();
        let tail_text = &s[split_byte..];
        if any_escape_before_split {
            head.push_str(RESET);
            let mut tail = last_escape;
            tail.push_str(tail_text);
            (head, tail)
        } else {
            (head, tail_text.to_string())
        }
    } else {
        // Everything fits
        (s.to_string(), String::new())
    }
}

/// Word-wrap every cell in `row` to its column width.
/// Returns a `Vec<Vec<Vec<String>>>`: `[col_index][wrap_line_index] = text`.
fn wrap_row(row: &[String], col_widths: &[usize], col_count: usize) -> Vec<Vec<String>> {
    (0..col_count)
        .map(|i| {
            let cell = row.get(i).map(String::as_str).unwrap_or("");
            wrap_cell(cell, col_widths[i])
        })
        .collect()
}

/// Emit the output lines for one logical table row.
///
/// `wrapped[col]` is the list of wrapped lines for that column.  All columns
/// are padded to the same height (the max wrap depth across columns), so
/// borders always align.
fn emit_row_lines(
    wrapped: &[Vec<String>],
    col_widths: &[usize],
    col_count: usize,
    is_header: bool,
    out: &mut Vec<String>,
    border: &str,
) {
    let height = wrapped.iter().map(|c| c.len()).max().unwrap_or(1);
    for line_idx in 0..height {
        let mut line = format!("{}│{}", border, RESET);
        for col_i in 0..col_count {
            let col_w = col_widths[col_i];
            let text = wrapped[col_i].get(line_idx).map(String::as_str).unwrap_or("");
            let text_w = visual_len(text);
            let pad = col_w.saturating_sub(text_w);
            if is_header {
                line.push_str(&format!(" {}{}{}{} ",
                    BOLD, text, RESET, " ".repeat(pad)));
            } else {
                line.push_str(&format!(" {}{} ",
                    text, " ".repeat(pad)));
            }
            line.push_str(&format!("{}│{}", border, RESET));
        }
        out.push(line);
    }
}


// ─── termimad renderer (legacy) ──────────────────────────────────────────────

/// Legacy renderer that delegates to `termimad`.
#[allow(dead_code)] // Available as an alternative renderer via renderer_from_name("termimad")
pub(crate) struct TermimadRenderer;

impl MarkdownRenderer for TermimadRenderer {
    fn render(&self, markdown: &str, term_width: usize) -> Vec<String> {
        use termimad::{Area, MadSkin};
        let skin = MadSkin::default();
        let area = Area::new(0, 0, term_width as u16, u16::MAX);
        let fmt_text = skin.area_text(markdown, &area);
        fmt_text
            .to_string()
            .lines()
            .map(|l| l.to_string())
            .collect()
    }
}

// ─── Width helpers ────────────────────────────────────────────────────────────

/// Visual column count of a string that may contain ANSI escape sequences.
pub(crate) fn visual_len(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    let mut width = 0usize;
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        width += ch.width().unwrap_or(1);
    }
    width
}


// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pulldown_renders_headings() {
        let md = "# Hello\n\nSome text.\n";
        let lines = PulldownRenderer.render(md, 80);
        let joined = lines.join("\n");
        assert!(joined.contains("Hello"), "heading text should be present");
        // Should have ANSI colour for H1
        assert!(joined.contains(H1), "H1 colour code expected");
    }

    #[test]
    fn test_pulldown_renders_bold_italic() {
        let lines = PulldownRenderer.render("**bold** and _italic_", 80);
        let joined = lines.join("\n");
        assert!(joined.contains("bold"));
        assert!(joined.contains("italic"));
        assert!(joined.contains(BOLD));
        assert!(joined.contains(ITALIC));
    }

    #[test]
    fn test_pulldown_renders_code_block() {
        let md = "```rust\nfn main() {}\n```\n";
        let lines = PulldownRenderer.render(md, 80);
        let joined = lines.join("\n");
        assert!(joined.contains("fn main()"));
        assert!(joined.contains("rust"));
    }

    #[test]
    fn test_code_block_content_between_borders() {
        // Opening ╔ border must come before code content; closing ╚ must come after.
        let md = "```sh\necho hello\necho world\n```\n";
        let lines = PulldownRenderer.render(md, 80);
        let plain: Vec<String> = lines.iter().map(|l| strip_ansi(l)).collect();

        let open_idx  = plain.iter().position(|l| l.contains('╔')).expect("opening border");
        let close_idx = plain.iter().position(|l| l.contains('╚')).expect("closing border");
        let hello_idx = plain.iter().position(|l| l.contains("echo hello")).expect("first code line");
        let world_idx = plain.iter().position(|l| l.contains("echo world")).expect("second code line");

        assert!(open_idx  < hello_idx, "opening border must precede first code line");
        assert!(hello_idx < world_idx, "first code line must precede second");
        assert!(world_idx < close_idx, "closing border must follow all code lines");
    }

    #[test]
    fn test_code_block_box_geometry() {
        // Every line of the code block must be exactly term_width wide.
        // Top border:     starts with ╔, ends with ╗
        // Content lines:  start with ║, end with ║
        // Bottom border:  starts with ╚, ends with ╝
        let md = "```rust\nlet x = 1;\nlet y = 2;\n```\n";
        let term_w = 60usize;
        let lines = PulldownRenderer.render(md, term_w);
        let plain: Vec<String> = lines.iter().map(|l| strip_ansi(l)).collect();

        let open_idx  = plain.iter().position(|l| l.starts_with('╔')).expect("top border");
        let close_idx = plain.iter().position(|l| l.starts_with('╚')).expect("bottom border");

        // Top border
        let top = &plain[open_idx];
        assert!(top.ends_with('╗'), "top border must end with ╗: {top:?}");
        assert_eq!(visual_len(&lines[open_idx]), term_w,
            "top border must be exactly term_w wide");

        // Bottom border
        let bot = &plain[close_idx];
        assert!(bot.ends_with('╝'), "bottom border must end with ╝: {bot:?}");
        assert_eq!(visual_len(&lines[close_idx]), term_w,
            "bottom border must be exactly term_w wide");

        // Content lines
        for i in (open_idx + 1)..close_idx {
            let l = &plain[i];
            assert!(l.starts_with('║'), "content line must start with ║: {l:?}");
            assert!(l.ends_with('║'),   "content line must end with ║: {l:?}");
            assert_eq!(visual_len(&lines[i]), term_w,
                "content line {i} must be exactly term_w wide");
        }
    }

    #[test]
    fn test_code_block_long_line_wraps_inside_borders() {
        // A line longer than term_width must wrap; all wrapped pieces must appear
        // between the opening and closing borders.
        let long_line = "x".repeat(200);
        let md = format!("```\n{}\n```\n", long_line);
        let lines = PulldownRenderer.render(&md, 40);
        let plain: Vec<String> = lines.iter().map(|l| strip_ansi(l)).collect();

        let open_idx  = plain.iter().position(|l| l.contains('╔')).expect("opening border");
        let close_idx = plain.iter().position(|l| l.contains('╚')).expect("closing border");

        for (i, l) in plain.iter().enumerate() {
            if l.contains('x') {
                assert!(i > open_idx,  "code content at line {i} must be after opening border");
                assert!(i < close_idx, "code content at line {i} must be before closing border");
            }
        }
        let content_lines: Vec<&str> = plain[open_idx+1..close_idx].iter()
            .map(String::as_str)
            .filter(|l| l.contains('x'))
            .collect();
        assert!(content_lines.len() >= 2, "long line should wrap into multiple display lines");
    }

    #[test]
    fn test_pulldown_renders_bullet_list() {
        let md = "- item one\n- item two\n";
        let lines = PulldownRenderer.render(md, 80);
        let joined = lines.join("\n");
        assert!(joined.contains("item one"));
        assert!(joined.contains("item two"));
        assert!(joined.contains("•"));
    }

    #[test]
    fn test_pulldown_renders_ordered_list() {
        let md = "1. first\n2. second\n";
        let lines = PulldownRenderer.render(md, 80);
        let joined = lines.join("\n");
        assert!(joined.contains("first"));
        assert!(joined.contains("second"));
    }

    #[test]
    fn test_pulldown_renders_table() {
        let md = "| Key | Action |\n|-----|--------|\n| Up | Move up |\n";
        let lines = PulldownRenderer.render(md, 80);
        let joined = lines.join("\n");
        assert!(joined.contains("Key"), "header cell");
        assert!(joined.contains("Move up"), "body cell");
        assert!(!joined.contains("|--"), "raw separator should be gone");
    }

    #[test]
    fn test_table_cell_wraps_when_column_is_narrow() {
        // Two-column table rendered into a narrow terminal so the second column wraps
        let md = "| A | Description |\n|---|-------------|\n| x | This is a long description that should wrap |\n";
        let lines = PulldownRenderer.render(md, 40);
        let joined = lines.join("\n");
        // The long description must appear somewhere (possibly split across lines)
        assert!(joined.contains("This"), "wrapped content must appear");
        assert!(joined.contains("long"), "wrapped content must appear");
        // No line should be wider than ~40 visible columns
        for line in &lines {
            let vw = visual_len(line);
            assert!(vw <= 44, "line too wide ({vw}): {:?}", strip_ansi(line));
        }
    }

    #[test]
    fn test_table_all_columns_same_height_after_wrap() {
        let md = "| Short | Very long content that will definitely need to wrap across several lines when the terminal is narrow |\n\
                  |-------|-------|\n\
                  | ok | also long content here that wraps |\n";
        let lines = PulldownRenderer.render(md, 40);
        // Every output line that is a content row (not a border) must have the same
        // visible structure — i.e., start and end with a │ character.
        let content_lines: Vec<&str> = lines.iter()
            .map(String::as_str)
            // Strip leading ANSI before checking the first visible char
            .filter(|l| {
                let stripped = strip_ansi(l);
                stripped.starts_with('│')
            })
            .collect();
        assert!(!content_lines.is_empty(), "should have content lines");
        for l in &content_lines {
            let stripped = strip_ansi(l);
            assert!(stripped.ends_with('│'), "row line should end with │: {stripped:?}");
        }
    }

    #[test]
    fn test_wrap_cell_basic() {
        let lines = wrap_cell("hello world foo bar", 10);
        for l in &lines {
            assert!(visual_len(l) <= 10, "line too wide: {l:?}");
        }
        let joined = lines.join(" ");
        assert!(joined.contains("hello"));
        assert!(joined.contains("world"));
        assert!(joined.contains("foo"));
        assert!(joined.contains("bar"));
    }

    #[test]
    fn test_wrap_cell_single_word_hard_breaks() {
        let lines = wrap_cell("verylongwordthatexceedswidth", 8);
        for l in &lines {
            assert!(visual_len(l) <= 8, "line too wide: {l:?}");
        }
        let joined = lines.join("");
        assert_eq!(joined, "verylongwordthatexceedswidth");
    }

    #[test]
    fn test_wrap_cell_empty() {
        let lines = wrap_cell("", 10);
        assert_eq!(lines, vec![""]);
    }

    #[test]
    fn test_wrap_cell_fits_in_one_line() {
        let lines = wrap_cell("short", 20);
        assert_eq!(lines, vec!["short"]);
    }

    #[test]
    fn test_split_at_visual_width_plain() {
        let (head, tail) = split_at_visual_width("hello world", 5);
        assert_eq!(head, "hello");
        assert_eq!(tail, " world");
    }

    #[test]
    fn test_split_at_visual_width_fits_entirely() {
        let (head, tail) = split_at_visual_width("hi", 10);
        assert_eq!(head, "hi");
        assert_eq!(tail, "");
    }

    #[test]
    fn test_split_at_visual_width_ansi_reset_not_leaked() {
        // An ANSI RESET sequence must not appear as raw "[0m" in the tail.
        // If the split falls after an escape, the escape must be re-emitted on tail.
        let styled = format!("{}bold{} plain", BOLD, RESET);
        let (head, tail) = split_at_visual_width(&styled, 4); // split inside "bold"
        // head must not contain bare "[" — i.e. no split mid-escape
        assert!(!head.contains("[0m") || head.contains("\x1b[0m"),
            "bare [0m in head: {head:?}");
        assert!(!tail.contains("[0m") || tail.contains("\x1b[0m"),
            "bare [0m in tail: {tail:?}");
        // Reassembling must contain all visible text
        let all = format!("{}{}", head, tail);
        assert!(all.contains("bold"), "text preserved: {all:?}");
    }

    #[test]
    fn test_split_at_visual_width_open_style_closed_in_head() {
        // When style is open at the split point, head should end with RESET.
        let styled = format!("{}ABCDE{} rest", BOLD, RESET);
        // Split at 3 — inside "ABCDE", while BOLD is still open
        let (head, _tail) = split_at_visual_width(&styled, 3);
        assert!(head.ends_with(RESET),
            "head should close open style with RESET: {head:?}");
    }

    #[test]
    fn test_split_at_visual_width_style_reopened_in_tail() {
        // The active style must be re-emitted at the start of tail.
        let styled = format!("{}ABCDE{} rest", BOLD, RESET);
        let (_head, tail) = split_at_visual_width(&styled, 3);
        assert!(tail.starts_with(BOLD),
            "tail should reopen BOLD: {tail:?}");
    }

    #[test]
    fn test_wrap_cell_with_ansi_no_bare_escapes() {
        // Inline code in a very narrow column must not leak bare "[0m" sequences.
        let cell = format!("{}{} inline_code_word_here {}", CODE_BG, CODE_FG, RESET);
        let lines = wrap_cell(&cell, 6);
        for line in &lines {
            assert!(!line.contains("[0m") || line.contains("\x1b[0m"),
                "bare escape fragment in line: {line:?}");
            assert!(!line.contains("[38;") || line.contains("\x1b[38;"),
                "bare escape fragment in line: {line:?}");
        }
    }

    /// Helper used only in tests: strip ANSI escape codes from a string.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut in_esc = false;
        for ch in s.chars() {
            if ch == '\x1b' { in_esc = true; continue; }
            if in_esc { if ch.is_ascii_alphabetic() { in_esc = false; } continue; }
            out.push(ch);
        }
        out
    }

    #[test]
    fn test_renderer_from_name_pulldown() {
        let r = renderer_from_name("pulldown");
        let lines = r.render("hello", 80);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_renderer_from_name_termimad() {
        let r = renderer_from_name("termimad");
        let lines = r.render("hello", 80);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_renderer_from_name_unknown_falls_back_to_pulldown() {
        let r = renderer_from_name("unknown_renderer");
        let lines = r.render("# Test", 80);
        let joined = lines.join("\n");
        assert!(joined.contains("Test"));
    }

    #[test]
    fn test_visual_len_ignores_ansi() {
        let s = "\x1b[1mHello\x1b[0m";
        assert_eq!(visual_len(s), 5);
    }

    #[test]
    fn test_visual_len_plain_text() {
        assert_eq!(visual_len("Hello"), 5);
    }

    #[test]
    fn test_horizontal_rule() {
        let lines = PulldownRenderer.render("---\n", 20);
        let joined = lines.join("\n");
        assert!(joined.contains("─"));
    }

    #[test]
    fn test_inline_code() {
        let lines = PulldownRenderer.render("Use `foo()` here.", 80);
        let joined = lines.join("\n");
        assert!(joined.contains("foo()"));
    }

    #[test]
    fn test_no_raw_table_separators_in_output() {
        let settings = crate::settings::Settings::default();
        let content = include_str!("../defaults/help-editor.md");
        let replaced = super::super::help::replace_keybindings_pub(content, &settings);
        let lines = PulldownRenderer.render(&replaced, 100);
        let joined = lines.join("\n");
        assert!(!joined.contains("|--"), "raw table separator should not appear");
        assert!(!joined.contains("--|"), "raw table separator should not appear");
    }

    #[test]
    fn test_heading_underline_is_indented() {
        // H1 and H2 underlines must start at the same column as the heading text.
        for md in ["# My Title\n", "## My Section\n"] {
            let lines = PulldownRenderer.render(md, 80);
            let heading_plain = lines.iter()
                .map(|l| strip_ansi(l))
                .find(|s| s.contains("My "))
                .expect("heading text should appear");
            let underline_plain = lines.iter()
                .map(|l| strip_ansi(l))
                .find(|s| s.contains('═') || s.contains('─'))
                .expect("underline should appear");

            let h_indent: String = heading_plain.chars().take_while(|c| *c == ' ').collect();
            let u_indent: String = underline_plain.chars().take_while(|c| *c == ' ').collect();
            assert!(!h_indent.is_empty(), "heading should be indented: {heading_plain:?}");
            assert_eq!(h_indent, u_indent,
                "underline indent {u_indent:?} should match heading indent {h_indent:?}");
        }
    }

    #[test]
    fn test_list_wrapped_lines_are_indented() {
        // A long bullet item must have its continuation lines indented to align
        // with the text start (after "• "), not flush with column 0.
        let md = "- Short\n- This item is intentionally very long so that it wraps when rendered at forty columns wide in the terminal\n";
        let lines = PulldownRenderer.render(md, 40);
        let non_empty: Vec<String> = lines.iter()
            .map(|l| strip_ansi(l))
            .filter(|s| !s.trim().is_empty())
            .collect();

        assert!(non_empty.len() >= 2, "long item should produce multiple lines");

        // Find the long-item lines: those after the "• Short" line
        let bullet_idx = non_empty.iter().position(|s| s.contains('•')).unwrap_or(0);
        let item_lines: Vec<&str> = non_empty[bullet_idx..].iter()
            .map(String::as_str)
            .collect();

        assert!(item_lines[0].contains('•'), "first line must have bullet");

        // Any continuation line (no bullet, not blank) must start with a space
        for cont in item_lines.iter().skip(1) {
            if cont.trim().is_empty() || cont.contains('•') { break; }
            assert!(cont.starts_with(' '),
                "continuation line should be indented: {cont:?}");
        }
    }
}

