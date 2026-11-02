//! Parts of a note that links can point at: a heading's section
//! (`[[Note#Heading]]`) or a block (`[[Note#^id]]`), as in Obsidian.
//!
//! A block is a paragraph or a list item (with the items nested under it)
//! whose last line ends with ` ^id`, or the block just above a line that
//! holds only `^id` (how a table, quote or code block gets an id).

/// The Markdown a link anchor points at in `body`, or `None` when the
/// heading or block is not there. The anchor is what follows `#` in the
/// link: a heading text (`Power`, or `Parts#Motors` for a nested heading)
/// or a block id (`^wiring`). Block ids are left out of the returned text.
pub fn section(body: &str, anchor: &str) -> Option<String> {
    let anchor = anchor.trim();
    if let Some(id) = anchor.strip_prefix('^') {
        block(body, id)
    } else {
        heading_section(body, anchor)
    }
}

/// The block ids defined in a note, in order.
pub fn block_ids(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines_outside_code(body) {
        if let Some(id) = trailing_block_id(line.text) {
            out.push(id.to_string());
        }
    }
    out
}

/// `text ^abc-1` → `Some("abc-1")`; also a line that is only `^abc-1`.
pub fn trailing_block_id(line: &str) -> Option<&str> {
    let t = line.trim_end();
    let caret = t.rfind('^')?;
    let id = &t[caret + 1..];
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let before = &t[..caret];
    if before.trim().is_empty() || before.ends_with(' ') || before.ends_with('\t') {
        Some(id)
    } else {
        None
    }
}

/// The line without its trailing ` ^id`.
pub fn strip_block_id(line: &str) -> &str {
    match trailing_block_id(line) {
        Some(id) => line.trim_end()[..line.trim_end().len() - id.len() - 1].trim_end(),
        None => line,
    }
}

struct Line<'a> {
    index: usize,
    text: &'a str,
}

/// Lines with fenced code blocks left out (an `^id` inside code is code).
fn lines_outside_code(body: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for (index, text) in body.lines().enumerate() {
        let t = text.trim_start();
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") {
            fence = Some("```");
            continue;
        }
        if t.starts_with("~~~") {
            fence = Some("~~~");
            continue;
        }
        out.push(Line { index, text });
    }
    out
}

fn heading_of(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    if line.len() - t.len() > 3 {
        return None;
    }
    let level = t.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &t[level..];
    if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim_end();
    Some((level, text))
}

fn normalise(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn heading_section(body: &str, anchor: &str) -> Option<String> {
    let all: Vec<&str> = body.lines().collect();
    let candidates = lines_outside_code(body);
    // `Parts#Motors`: each part must match a heading nested inside the previous one.
    let parts: Vec<String> = anchor.split('#').map(normalise).filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    let mut from = 0usize;
    let mut within: Option<usize> = None; // level of the enclosing heading
    let mut found: Option<(usize, usize)> = None; // (line index, level)
    for part in &parts {
        found = None;
        for line in candidates.iter().filter(|l| l.index >= from) {
            if let Some((level, text)) = heading_of(line.text) {
                if let Some(outer) = within {
                    if level <= outer && line.index > from {
                        break;
                    }
                }
                if normalise(strip_block_id(text)) == *part {
                    found = Some((line.index, level));
                    break;
                }
            }
        }
        let (index, level) = found?;
        from = index + 1;
        within = Some(level);
    }
    let (start, level) = found?;
    let end = candidates
        .iter()
        .filter(|l| l.index > start)
        .find(|l| heading_of(l.text).is_some_and(|(lv, _)| lv <= level))
        .map(|l| l.index)
        .unwrap_or(all.len());
    Some(trim_lines(&all[start..end]))
}

fn block(body: &str, id: &str) -> Option<String> {
    let all: Vec<&str> = body.lines().collect();
    let line = lines_outside_code(body)
        .into_iter()
        .find(|l| trailing_block_id(l.text) == Some(id))?;
    let i = line.index;
    // `^id` on a line of its own names the block above it.
    if line.text.trim() == format!("^{id}") {
        let end = i;
        let mut start = end;
        while start > 0 && !all[start - 1].trim().is_empty() {
            start -= 1;
        }
        // A fenced block above: take it whole.
        if start == end && end > 0 {
            return None;
        }
        return Some(trim_lines(&all[start..end]));
    }
    let indent = |s: &str| s.len() - s.trim_start().len();
    let is_item = |s: &str| {
        let t = s.trim_start();
        t.starts_with("- ")
            || t.starts_with("* ")
            || t.starts_with("+ ")
            || t.split_once(". ")
                .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    if is_item(all[i]) {
        // The item and everything indented under it.
        let base = indent(all[i]);
        let mut end = i + 1;
        while end < all.len() && !all[end].trim().is_empty() && indent(all[end]) > base {
            end += 1;
        }
        let mut lines: Vec<String> = all[i..end]
            .iter()
            .map(|l| l[base.min(indent(l))..].to_string())
            .collect();
        lines[0] = strip_block_id(&lines[0]).to_string();
        return Some(lines.join("\n"));
    }
    // A paragraph: back to the blank line above.
    let mut start = i;
    while start > 0 && !all[start - 1].trim().is_empty() && heading_of(all[start - 1]).is_none() {
        start -= 1;
    }
    let mut lines: Vec<&str> = all[start..=i].to_vec();
    let last = lines.len() - 1;
    lines[last] = strip_block_id(lines[last]);
    Some(trim_lines(&lines))
}

fn trim_lines(lines: &[&str]) -> String {
    let text = lines.join("\n");
    text.trim_matches('\n').trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "# Garden Robot\n\nIntro text.\n\n## Power\n\nBattery is 12 V. ^battery\n\n### Motors\n\nTwo DC motors.\n\n## Parts\n\n- wheels ^wheels\n  - 65 mm\n- frame\n\n| a | b |\n| - | - |\n| 1 | 2 |\n^table\n\n```\nnot ^code\n```\n";

    #[test]
    fn heading_sections_run_to_the_next_heading_of_the_same_level() {
        assert_eq!(
            section(BODY, "Power").unwrap(),
            "## Power\n\nBattery is 12 V. ^battery\n\n### Motors\n\nTwo DC motors."
        );
        assert_eq!(section(BODY, "motors").unwrap(), "### Motors\n\nTwo DC motors.");
        assert_eq!(section(BODY, "Power#Motors").unwrap(), "### Motors\n\nTwo DC motors.");
        assert!(section(BODY, "Parts#Motors").is_none());
        assert!(section(BODY, "Nope").is_none());
        assert!(section(BODY, "Parts").unwrap().ends_with("```"));
    }

    #[test]
    fn blocks_are_paragraphs_items_or_the_block_above_a_bare_id() {
        assert_eq!(section(BODY, "^battery").unwrap(), "Battery is 12 V.");
        assert_eq!(section(BODY, "^wheels").unwrap(), "- wheels\n  - 65 mm");
        assert_eq!(section(BODY, "^table").unwrap(), "| a | b |\n| - | - |\n| 1 | 2 |");
        assert!(section(BODY, "^code").is_none());
        assert_eq!(block_ids(BODY), vec!["battery", "wheels", "table"]);
    }

    #[test]
    fn block_id_rules() {
        assert_eq!(trailing_block_id("text ^ab-1"), Some("ab-1"));
        assert_eq!(trailing_block_id("^ab"), Some("ab"));
        assert_eq!(trailing_block_id("x^2"), None);
        assert_eq!(trailing_block_id("text ^a_b"), None);
        assert_eq!(strip_block_id("text ^ab  "), "text");
        assert_eq!(strip_block_id("plain"), "plain");
    }
}
