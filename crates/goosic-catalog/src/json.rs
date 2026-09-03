//! Small, dependency-free helpers for walking InnerTube's deeply nested JSON.
//!
//! The upstream layout changes without notice, so this crate never follows a long fixed path.
//! It collects renderer nodes by key wherever they appear and reads the fields it understands.

use serde_json::Value;

/// Collects every object stored under `key`, anywhere in the tree, in document order.
///
/// Recursion stops descending into a node once it has been collected, so nested shelves of the
/// same kind do not produce duplicated ancestors.
pub fn collect<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    let mut found = Vec::new();
    walk(value, key, &mut found);
    found
}

fn walk<'a>(value: &'a Value, key: &str, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            for (name, child) in map {
                if name == key {
                    found.push(child);
                } else {
                    walk(child, key, found);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, key, found);
            }
        }
        _ => {}
    }
}

/// Returns the first object stored under `key` anywhere in the tree.
pub fn first<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for (name, child) in map {
                if name == key {
                    return Some(child);
                }
                if let Some(hit) = first(child, key) {
                    return Some(hit);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| first(item, key)),
        _ => None,
    }
}

/// Follows a literal path of object keys and array indices.
pub fn path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = value;
    for segment in path {
        cursor = match segment.parse::<usize>() {
            Ok(index) => cursor.get(index)?,
            Err(_) => cursor.get(segment)?,
        };
    }
    Some(cursor)
}

/// Joins a renderer's `runs` into one display string.
///
/// Accepts either a node that *is* `{runs: [...]}` or one that contains it, and falls back to
/// `simpleText` because YTM still emits it for some headers.
pub fn runs_text(value: &Value) -> String {
    if let Some(runs) = value.get("runs").and_then(Value::as_array) {
        return runs
            .iter()
            .filter_map(|run| run.get("text").and_then(Value::as_str))
            .collect::<String>();
    }
    if let Some(text) = value.get("simpleText").and_then(Value::as_str) {
        return text.to_owned();
    }
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    String::new()
}

/// The individual `runs` entries of a text node, if any.
pub fn runs(value: &Value) -> &[Value] {
    value
        .get("runs")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Picks the widest thumbnail that is still reasonable to fetch for a list row.
///
/// Upstream returns ascending sizes; anything past `max_width` is wasted bandwidth in a shell
/// that renders small artwork.
pub fn thumbnail(value: &Value, max_width: u64) -> Option<String> {
    let mut best: Option<(u64, &str)> = None;
    for list in collect(value, "thumbnails") {
        for entry in list.as_array()?.iter() {
            let url = entry.get("url").and_then(Value::as_str)?;
            let width = entry.get("width").and_then(Value::as_u64).unwrap_or(0);
            let better = match best {
                None => true,
                Some((current, _)) if current > max_width => width < current,
                Some((current, _)) => width > current && width <= max_width,
            };
            if better {
                best = Some((width, url));
            }
        }
    }
    best.map(|(_, url)| url.to_owned())
}

/// `true` when the text looks like `m:ss`, `mm:ss`, or `h:mm:ss`.
pub fn is_duration(text: &str) -> bool {
    let mut parts = text.split(':').peekable();
    let mut count = 0;
    while let Some(part) = parts.next() {
        count += 1;
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        // Only the leading field may be a single digit; the rest are zero padded.
        if count > 1 && part.len() != 2 {
            return false;
        }
        if parts.peek().is_none() {
            break;
        }
    }
    (2..=3).contains(&count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collect_finds_nested_renderers_without_duplicating_ancestors() {
        let value = json!({
            "a": {"row": {"row": {"title": "inner"}}},
            "b": [{"row": {"title": "second"}}],
        });
        let found = collect(&value, "row");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0]["row"]["title"], "inner");
        assert_eq!(found[1]["title"], "second");
    }

    #[test]
    fn runs_text_joins_runs_and_falls_back_to_simple_text() {
        assert_eq!(
            runs_text(&json!({"runs": [{"text": "Song"}, {"text": " • "}, {"text": "Artist"}]})),
            "Song • Artist"
        );
        assert_eq!(runs_text(&json!({"simpleText": "Album"})), "Album");
        assert_eq!(runs_text(&json!({})), "");
    }

    #[test]
    fn thumbnail_prefers_the_widest_entry_within_the_budget() {
        let value = json!({"thumbnails": [
            {"url": "small", "width": 60, "height": 60},
            {"url": "good", "width": 226, "height": 226},
            {"url": "huge", "width": 1080, "height": 1080},
        ]});
        assert_eq!(thumbnail(&value, 400).as_deref(), Some("good"));
    }

    #[test]
    fn thumbnail_falls_back_to_the_smallest_when_all_exceed_the_budget() {
        let value = json!({"thumbnails": [
            {"url": "big", "width": 1080, "height": 1080},
            {"url": "bigger", "width": 2160, "height": 2160},
        ]});
        assert_eq!(thumbnail(&value, 400).as_deref(), Some("big"));
    }

    #[test]
    fn duration_shapes_are_recognized() {
        assert!(is_duration("3:42"));
        assert!(is_duration("12:07"));
        assert!(is_duration("1:02:33"));
        assert!(!is_duration("Song"));
        assert!(!is_duration("3:4"));
        assert!(!is_duration(""));
        assert!(!is_duration("2020"));
    }
}
