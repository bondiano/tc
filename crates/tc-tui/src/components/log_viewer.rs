use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, FocusPanel};
use crate::log_view::LogView;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let focused = app.focus == FocusPanel::Log;
    let inner_height = area.height.saturating_sub(2);
    app.log_view.viewport_height.set(inner_height);

    let title = build_title(&app.log_view, focused);
    let block = Block::default().borders(Borders::ALL).title(title);

    let view = &app.log_view;
    let start = view.offset;
    let end = (start + inner_height as usize).min(view.lines.len());
    let current_match = view.current_match_line();
    let search = view.search.as_str();

    let lines: Vec<Line> = view.lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let line_idx = start + i;
            let is_current = current_match == Some(line_idx);
            style_line(raw, search, is_current)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn build_title(view: &LogView, focused: bool) -> String {
    let mut bits: Vec<String> = Vec::new();
    if !view.lines.is_empty() {
        bits.push(format!("{}/{}", view.offset + 1, view.lines.len()));
    }
    if !view.search.is_empty() {
        let total = view.matches.len();
        let current = view.match_cursor.map(|i| i + 1).unwrap_or(0);
        bits.push(format!("/{} [{}/{}]", view.search, current, total));
    }
    if view.follow {
        bits.push("follow".into());
    }
    let suffix = if bits.is_empty() {
        String::new()
    } else {
        format!(" -- {}", bits.join(" "))
    };
    if focused {
        format!("[ Log (l){suffix} ]")
    } else {
        format!(" Log (l){suffix} ")
    }
}

/// Render a log line with inline markdown styling, then overlay search-hit
/// highlighting on top. Search highlight always wins visually.
fn style_line<'a>(raw: &'a str, search: &str, is_current: bool) -> Line<'a> {
    let runs = markdown_runs(raw);
    if search.is_empty() {
        return Line::from(runs.into_iter().map(|(sp, _)| sp).collect::<Vec<_>>());
    }
    let hit_style = hit_style(is_current);
    Line::from(overlay_search(runs, raw, search, hit_style))
}

/// One entry from the markdown parser: a styled span and, when the span's
/// text is a literal slice of the input, the byte range it occupies inside
/// `raw`. Synthesized spans (like the bullet glyph) carry `None`, so
/// the search overlay knows to skip over them untouched.
type StyledRun<'a> = (Span<'a>, Option<(usize, usize)>);

fn hit_style(is_current: bool) -> Style {
    if is_current {
        Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(Color::LightYellow).fg(Color::Black)
    }
}

/// Parse a single log line into styled runs. Recognised syntax:
///
/// - Leading `# heading`, `## heading`, `### heading` -> yellow bold
/// - Leading `- `, `* `, `+ ` -> bullet glyph in green, rest inline-styled
/// - Leading `N. ` (1..9) -> cyan number gutter, rest inline-styled
/// - Leading `> ` -> dark gray italic
/// - Inline `` `code` `` -> cyan dim
/// - Inline `**bold**` -> bold
/// - Inline `*italic*` / `_italic_` -> italic
///
/// No links, no tables, no nested emphasis; this is intentionally small
/// and matches what typically shows up in tc log streams.
fn markdown_runs(raw: &str) -> Vec<StyledRun<'_>> {
    if let Some((hashes, tail)) = strip_prefix_heading(raw) {
        let style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        let hashes_end = hashes.len();
        let mut out: Vec<StyledRun<'_>> =
            vec![(Span::styled(hashes, style), Some((0, hashes_end)))];
        out.extend(inline_runs(tail, style, hashes_end));
        return out;
    }
    if let Some((prefix, rest)) = strip_prefix_bullet(raw) {
        let bullet = Span::styled(prefix, Style::default().fg(Color::Green));
        // prefix is a replacement glyph, not a slice of `raw` -- record None.
        let consumed = raw.len() - rest.len();
        let mut out: Vec<StyledRun<'_>> = vec![(bullet, None)];
        out.extend(inline_runs(rest, Style::default(), consumed));
        return out;
    }
    if let Some((prefix, rest)) = strip_prefix_numbered(raw) {
        let gutter = Span::styled(prefix, Style::default().fg(Color::Cyan));
        let end = prefix.len();
        let mut out: Vec<StyledRun<'_>> = vec![(gutter, Some((0, end)))];
        out.extend(inline_runs(rest, Style::default(), end));
        return out;
    }
    if let Some(rest) = raw.strip_prefix("> ") {
        let style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);
        let mut out: Vec<StyledRun<'_>> = vec![(Span::styled(&raw[..2], style), Some((0, 2)))];
        out.extend(inline_runs(rest, style, 2));
        return out;
    }
    inline_runs(raw, Style::default(), 0)
}

/// Returns (hash-prefix-including-space, remainder) for `# `, `## `, `### `.
fn strip_prefix_heading(raw: &str) -> Option<(&str, &str)> {
    'level: for level in (1..=3).rev() {
        let hashes = &"###"[..level];
        if let Some(rest) = raw.strip_prefix(hashes)
            && rest.starts_with(' ')
        {
            let split = level + 1;
            return Some((&raw[..split], &raw[split..]));
        }
        continue 'level;
    }
    None
}

/// Map `- `, `* `, `+ ` to a bullet gutter span; returns (gutter-span-content, remainder).
fn strip_prefix_bullet(raw: &str) -> Option<(&'static str, &str)> {
    if let Some(rest) = raw.strip_prefix("- ") {
        return Some(("- ", rest));
    }
    if let Some(rest) = raw.strip_prefix("* ") {
        return Some(("- ", rest));
    }
    if let Some(rest) = raw.strip_prefix("+ ") {
        return Some(("- ", rest));
    }
    None
}

/// Accept `1. `..`9. ` as a numbered list prefix; returns (gutter, rest).
fn strip_prefix_numbered(raw: &str) -> Option<(&str, &str)> {
    let bytes = raw.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    if !bytes[0].is_ascii_digit() || bytes[1] != b'.' || bytes[2] != b' ' {
        return None;
    }
    Some((&raw[..3], &raw[3..]))
}

/// Scan an inline string and produce styled runs. `raw_offset` is the
/// position of `raw` inside the original log line -- added to every emitted
/// range so the search overlay knows where to cut.
fn inline_runs(raw: &str, base: Style, raw_offset: usize) -> Vec<StyledRun<'_>> {
    let bytes = raw.as_bytes();
    let mut out: Vec<StyledRun<'_>> = Vec::new();
    let mut cursor = 0usize;
    let mut plain_start = 0usize;

    'scan: while cursor < bytes.len() {
        let b = bytes[cursor];
        match b {
            b'`' => {
                if let Some(end) = find_byte(bytes, b'`', cursor + 1) {
                    flush_plain(&mut out, raw, plain_start, cursor, base, raw_offset);
                    let style =
                        base.patch(Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM));
                    out.push((
                        Span::styled(&raw[cursor + 1..end], style),
                        Some((raw_offset + cursor + 1, raw_offset + end)),
                    ));
                    cursor = end + 1;
                    plain_start = cursor;
                    continue 'scan;
                }
            }
            b'*' => {
                if cursor + 1 < bytes.len()
                    && bytes[cursor + 1] == b'*'
                    && let Some(end) = find_double_asterisk(bytes, cursor + 2)
                {
                    flush_plain(&mut out, raw, plain_start, cursor, base, raw_offset);
                    let style = base.patch(Style::default().add_modifier(Modifier::BOLD));
                    out.push((
                        Span::styled(&raw[cursor + 2..end], style),
                        Some((raw_offset + cursor + 2, raw_offset + end)),
                    ));
                    cursor = end + 2;
                    plain_start = cursor;
                    continue 'scan;
                }
                if let Some(end) = find_byte(bytes, b'*', cursor + 1)
                    && end > cursor + 1
                    && !bytes[cursor + 1].is_ascii_whitespace()
                {
                    flush_plain(&mut out, raw, plain_start, cursor, base, raw_offset);
                    let style = base.patch(Style::default().add_modifier(Modifier::ITALIC));
                    out.push((
                        Span::styled(&raw[cursor + 1..end], style),
                        Some((raw_offset + cursor + 1, raw_offset + end)),
                    ));
                    cursor = end + 1;
                    plain_start = cursor;
                    continue 'scan;
                }
            }
            b'_' => {
                if let Some(end) = find_byte(bytes, b'_', cursor + 1)
                    && end > cursor + 1
                    && !bytes[cursor + 1].is_ascii_whitespace()
                    && is_emphasis_boundary(bytes, cursor)
                    && is_emphasis_boundary(bytes, end)
                {
                    flush_plain(&mut out, raw, plain_start, cursor, base, raw_offset);
                    let style = base.patch(Style::default().add_modifier(Modifier::ITALIC));
                    out.push((
                        Span::styled(&raw[cursor + 1..end], style),
                        Some((raw_offset + cursor + 1, raw_offset + end)),
                    ));
                    cursor = end + 1;
                    plain_start = cursor;
                    continue 'scan;
                }
            }
            _ => {}
        }
        cursor += 1;
        continue 'scan;
    }

    flush_plain(&mut out, raw, plain_start, bytes.len(), base, raw_offset);
    out
}

fn flush_plain<'a>(
    out: &mut Vec<StyledRun<'a>>,
    raw: &'a str,
    from: usize,
    to: usize,
    base: Style,
    raw_offset: usize,
) {
    if to > from {
        out.push((
            Span::styled(&raw[from..to], base),
            Some((raw_offset + from, raw_offset + to)),
        ));
    }
}

fn find_byte(bytes: &[u8], needle: u8, from: usize) -> Option<usize> {
    let mut i = from;
    'walk: while i < bytes.len() {
        if bytes[i] == needle {
            return Some(i);
        }
        i += 1;
        continue 'walk;
    }
    None
}

fn find_double_asterisk(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    'walk: while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'*' {
            return Some(i);
        }
        i += 1;
        continue 'walk;
    }
    None
}

/// A `_` acts as an emphasis delimiter only when it sits next to whitespace,
/// a non-alphanumeric boundary, or the string edge. Keeps `snake_case` intact.
fn is_emphasis_boundary(bytes: &[u8], pos: usize) -> bool {
    let prev_ok = pos == 0 || {
        let b = bytes[pos - 1];
        !(b.is_ascii_alphanumeric() || b == b'_')
    };
    let next_ok = pos + 1 >= bytes.len() || {
        let b = bytes[pos + 1];
        !(b.is_ascii_alphanumeric() || b == b'_')
    };
    prev_ok || next_ok
}

/// Walk the styled runs and, wherever a run has a known byte range in `raw`,
/// split it around any occurrences of `search` (case-insensitive), tagging
/// matches with the `hit` style. Synthesized runs (None range) pass through.
fn overlay_search<'a>(
    runs: Vec<StyledRun<'a>>,
    raw: &'a str,
    search: &str,
    hit: Style,
) -> Vec<Span<'a>> {
    let lower = raw.to_lowercase();
    let needle = search.to_lowercase();
    if !lower.contains(&needle) {
        return runs.into_iter().map(|(sp, _)| sp).collect();
    }

    let lower_bytes = lower.as_bytes();
    let mut out: Vec<Span<'a>> = Vec::with_capacity(runs.len());
    for (span, range) in runs {
        let Some((lo, hi)) = range else {
            out.push(span);
            continue;
        };
        let ctx = HitContext {
            raw,
            base: span.style,
            lower_bytes,
            needle: &needle,
            hit,
        };
        split_with_hits(&mut out, &ctx, lo, hi);
    }
    out
}

struct HitContext<'a, 'b> {
    raw: &'a str,
    base: Style,
    lower_bytes: &'b [u8],
    needle: &'b str,
    hit: Style,
}

fn split_with_hits<'a>(out: &mut Vec<Span<'a>>, ctx: &HitContext<'a, '_>, lo: usize, hi: usize) {
    let mut cursor = lo;
    'scan: while cursor < hi {
        let window = &ctx.lower_bytes[cursor..hi];
        let rel = window
            .windows(ctx.needle.len())
            .position(|w| w == ctx.needle.as_bytes());
        match rel {
            Some(r) => {
                let abs = cursor + r;
                let end = abs + ctx.needle.len();
                if abs > cursor {
                    out.push(Span::styled(&ctx.raw[cursor..abs], ctx.base));
                }
                out.push(Span::styled(&ctx.raw[abs..end], ctx.base.patch(ctx.hit)));
                cursor = end;
                if cursor >= hi {
                    break 'scan;
                }
            }
            None => {
                out.push(Span::styled(&ctx.raw[cursor..hi], ctx.base));
                break 'scan;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inline_runs_test(raw: &str, base: Style) -> Vec<Span<'_>> {
        inline_runs(raw, base, 0)
            .into_iter()
            .map(|(sp, _)| sp)
            .collect()
    }

    fn markdown_spans_test(raw: &str) -> Vec<Span<'_>> {
        markdown_runs(raw).into_iter().map(|(sp, _)| sp).collect()
    }

    #[test]
    fn title_contains_position_and_search() {
        let mut v = LogView::new();
        v.viewport_height.set(5);
        v.set_lines((0..10).map(|i| format!("item {i}")).collect());
        v.set_search("item 3".into());
        let t = build_title(&v, true);
        assert!(t.contains("/item 3"));
        assert!(t.contains("[1/1]"));
    }

    #[test]
    fn inline_bold_produces_bold_span() {
        let spans = inline_runs_test("a **b** c", Style::default());
        // Expect three spans: "a ", "b" (bold), " c".
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].content, "b");
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn inline_code_produces_cyan_dim() {
        let spans = inline_runs_test("`foo` bar", Style::default());
        assert_eq!(spans[0].content, "foo");
        assert!(spans[0].style.add_modifier.contains(Modifier::DIM));
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn inline_italic_star_and_underscore() {
        let s = inline_runs_test("*a* and _b_", Style::default());
        let italics: Vec<_> = s
            .iter()
            .filter(|sp| sp.style.add_modifier.contains(Modifier::ITALIC))
            .map(|sp| sp.content.as_ref())
            .collect();
        assert_eq!(italics, vec!["a", "b"]);
    }

    #[test]
    fn inline_underscore_skips_snake_case() {
        let s = inline_runs_test("snake_case_name", Style::default());
        // Either a single plain span, or multiple plain spans, but no italics.
        assert!(
            !s.iter()
                .any(|sp| sp.style.add_modifier.contains(Modifier::ITALIC))
        );
    }

    #[test]
    fn inline_unmatched_asterisk_stays_plain() {
        let s = inline_runs_test("5 * 3 = 15", Style::default());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].content, "5 * 3 = 15");
    }

    #[test]
    fn bullet_line_gets_gutter() {
        let spans = markdown_spans_test("- hello **world**");
        assert_eq!(spans[0].content, "- ");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        // rest contains bold span
        let bolds: Vec<_> = spans
            .iter()
            .filter(|sp| sp.style.add_modifier.contains(Modifier::BOLD))
            .collect();
        assert_eq!(bolds.len(), 1);
    }

    #[test]
    fn numbered_list_keeps_number() {
        let spans = markdown_spans_test("1. step one");
        assert_eq!(spans[0].content, "1. ");
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn heading_styles_with_yellow_bold() {
        let spans = markdown_spans_test("## Section");
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn quote_styles_dark_italic() {
        let spans = markdown_spans_test("> noted");
        assert!(spans[0].style.add_modifier.contains(Modifier::ITALIC));
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn plain_line_single_plain_span() {
        let spans = markdown_spans_test("just plain text");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "just plain text");
    }

    #[test]
    fn search_highlight_on_plain_line() {
        let line = style_line("hello WORLD world", "world", false);
        // Three plain segments: "hello ", then highlighted "WORLD", " ", highlighted "world".
        assert_eq!(line.spans.len(), 4);
        assert_eq!(line.spans[1].content, "WORLD");
        assert_eq!(line.spans[3].content, "world");
    }

    #[test]
    fn search_highlight_inside_bold_preserves_bold() {
        let line = style_line("plain **bold match** end", "match", false);
        // find the highlighted span; it should still have bold.
        let hit = line
            .spans
            .iter()
            .find(|sp| sp.content.as_ref() == "match")
            .expect("match span");
        assert!(hit.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(hit.style.bg, Some(Color::LightYellow));
    }

    #[test]
    fn search_highlight_on_bullet_keeps_gutter() {
        let line = style_line("- needle in haystack", "needle", false);
        // first span is still the bullet gutter, untouched.
        assert_eq!(line.spans[0].content, "- ");
        assert_eq!(line.spans[0].style.fg, Some(Color::Green));
        // highlighted "needle" exists somewhere.
        assert!(
            line.spans.iter().any(
                |sp| sp.content.as_ref() == "needle" && sp.style.bg == Some(Color::LightYellow)
            )
        );
    }

    #[test]
    fn search_highlight_current_match_uses_yellow() {
        let line = style_line("alpha beta", "beta", true);
        let hit = line
            .spans
            .iter()
            .find(|sp| sp.content.as_ref() == "beta")
            .expect("match");
        assert_eq!(hit.style.bg, Some(Color::Yellow));
    }

    #[test]
    fn search_misses_return_base_markdown() {
        let line = style_line("**bold**", "xyz", false);
        // "bold" should still be bold and no highlight.
        assert!(
            line.spans.iter().any(|sp| sp.content.as_ref() == "bold"
                && sp.style.add_modifier.contains(Modifier::BOLD))
        );
    }
}
