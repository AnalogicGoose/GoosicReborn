//! Read Lyricsfile 1.0 without inventing word boundaries or expanding YAML aliases.
//! Format authority: https://github.com/tranxuanthang/lyricsfile/blob/main/SPECIFICATION.md

use goosic_protocol::{LyricsDocument, LyricsLine, LyricsWord};
use serde::Deserialize;
use serde_json::{Map, Value};
use yaml_rust2::{
    parser::{Event, Parser},
    scanner::TScalarStyle,
    Yaml,
};

type Input<'a> = Parser<std::str::Chars<'a>>;

#[derive(Deserialize)]
struct Document {
    version: String,
    metadata: Metadata,
    lines: Option<Vec<Line>>,
    plain: Option<String>,
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(rename = "title")]
    _title: String,
    #[serde(rename = "artist")]
    _artist: String,
    instrumental: Option<bool>,
    offset_ms: Option<i64>,
    duration_ms: Option<i64>,
}

#[derive(Deserialize)]
struct Line {
    text: String,
    start_ms: i64,
    end_ms: Option<i64>,
    words: Option<Vec<Word>>,
}

#[derive(Deserialize)]
struct Word {
    text: String,
    start_ms: i64,
    end_ms: Option<i64>,
}

/// Syntax is restricted to the format's safe subset before typed deserialization.
fn next(parser: &mut Input<'_>, budget: &mut usize) -> Option<Event> {
    *budget = budget.checked_sub(1)?;
    parser.next_token().ok().map(|(event, _)| event)
}

fn value(parser: &mut Input<'_>, event: Event, depth: usize, budget: &mut usize) -> Option<Value> {
    if depth > 16 {
        return None;
    }
    match event {
        Event::Scalar(text, style, 0, None) => {
            if style != TScalarStyle::Plain {
                return Some(Value::String(text));
            }
            match Yaml::from_str(&text) {
                Yaml::Integer(integer) => Some(Value::Number(integer.into())),
                Yaml::Boolean(boolean) => Some(Value::Bool(boolean)),
                Yaml::Null => Some(Value::Null),
                Yaml::String(string) => Some(Value::String(string)),
                _ => None,
            }
        }
        Event::MappingStart(0, None) => {
            let mut map = Map::new();
            loop {
                let event = next(parser, budget)?;
                if event == Event::MappingEnd {
                    break;
                }
                let key = value(parser, event, depth + 1, budget)?
                    .as_str()?
                    .to_owned();
                if map.contains_key(&key) {
                    return None;
                }
                let event = next(parser, budget)?;
                map.insert(key, value(parser, event, depth + 1, budget)?);
            }
            Some(Value::Object(map))
        }
        Event::SequenceStart(0, None) => {
            let mut sequence = Vec::new();
            loop {
                let event = next(parser, budget)?;
                if event == Event::SequenceEnd {
                    break;
                }
                sequence.push(value(parser, event, depth + 1, budget)?);
            }
            Some(Value::Array(sequence))
        }
        // Anchors, aliases, tags and structural events cannot be lyric values.
        _ => None,
    }
}

fn read(source: &str) -> Option<Document> {
    if source.len() > 2 * 1024 * 1024 {
        return None;
    }
    let mut parser = Parser::new_from_str(source);
    let mut budget = 200_000;
    if next(&mut parser, &mut budget)? != Event::StreamStart
        || next(&mut parser, &mut budget)? != Event::DocumentStart
    {
        return None;
    }
    let event = next(&mut parser, &mut budget)?;
    let root = value(&mut parser, event, 0, &mut budget)?;
    if next(&mut parser, &mut budget)? != Event::DocumentEnd
        || next(&mut parser, &mut budget)? != Event::StreamEnd
    {
        return None;
    }
    serde_json::from_value(root).ok()
}

fn words(line: &Line) -> Vec<LyricsWord> {
    let Some(words) = &line.words else {
        return vec![];
    };
    if words.len() > 128
        || words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<String>()
            != line.text
    {
        return vec![];
    }
    let mut previous = line.start_ms;
    let mut result = Vec::new();
    for word in words {
        if word.text.is_empty()
            || word.start_ms < previous
            || word.end_ms.is_some_and(|end| end < word.start_ms)
            || line.end_ms.is_some_and(|end| {
                word.start_ms > end || word.end_ms.is_some_and(|word_end| word_end > end)
            })
        {
            return vec![];
        }
        previous = word.start_ms;
        result.push(LyricsWord {
            at_ms: word.start_ms,
            end_ms: word.end_ms,
            text: word.text.clone(),
        });
    }
    result
}

pub(crate) fn parse(source: &str) -> Option<LyricsDocument> {
    let document = read(source)?;
    if document.version != "1.0"
        || document
            .metadata
            .offset_ms
            .is_some_and(|offset| offset != 0)
        || document
            .metadata
            .duration_ms
            .is_some_and(|duration| duration < 0)
    {
        return None;
    }
    let lines = document.lines.unwrap_or_default();
    if document.metadata.instrumental.unwrap_or(false) {
        return (lines.is_empty() && document.plain.as_deref().unwrap_or("").trim().is_empty())
            .then(|| LyricsDocument {
                source: "LRCLIB".into(),
                synced: false,
                lines: vec![LyricsLine {
                    at_ms: -1,
                    text: "♪ Instrumental".into(),
                    words: vec![],
                }],
                truncated: false,
            });
    }
    if lines.is_empty() {
        let lines = crate::parse::parse_plain(document.plain.as_deref()?);
        return (!lines.is_empty()).then(|| crate::LyricsClient::clamp(lines, false));
    }
    if lines.len() > 2000
        || lines.iter().any(|line| {
            line.start_ms < 0
                || line.text.chars().count() > 512
                || line.end_ms.is_some_and(|end| end < line.start_ms)
        })
    {
        return None;
    }
    let mut lines: Vec<LyricsLine> = lines
        .iter()
        .map(|line| LyricsLine {
            at_ms: line.start_ms,
            text: line.text.clone(),
            words: words(line),
        })
        .collect();
    // Equal starts and overlapping vocal lines are valid in Lyricsfile.
    lines.sort_by_key(|line| line.at_ms);
    Some(crate::LyricsClient::clamp(lines, true))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const WORDS: &str = r#"version: '1.0'
metadata:
  title: Test song
  artist: Test artist
lines:
  - text: 'Hello world'
    start_ms: 1000
    end_ms: 3000
    words:
      - text: 'Hello '
        start_ms: 1000
        end_ms: 1800
      - text: world
        start_ms: 1800
        end_ms: 3000
"#;

    #[test]
    fn real_words_and_spacing_survive_conversion_and_wire_roundtrip() {
        let document = parse(WORDS).unwrap();
        assert_eq!(document.lines[0].text, "Hello world");
        let words = &document.lines[0].words;
        assert_eq!(words[0].at_ms, 1000);
        assert_eq!(words[0].end_ms, Some(1800));
        assert_eq!(words[1].text, "world");
        let wire = serde_json::to_vec(&document).unwrap();
        assert_eq!(
            serde_json::from_slice::<LyricsDocument>(&wire).unwrap(),
            document
        );
    }

    #[test]
    fn missing_word_ends_are_not_guessed() {
        let document = parse(
            &WORDS
                .replace("        end_ms: 1800\n", "")
                .replace("        end_ms: 3000\n", ""),
        )
        .unwrap();
        assert!(document.lines[0]
            .words
            .iter()
            .all(|word| word.end_ms.is_none()));
        let encoded = serde_json::to_string(&document).unwrap();
        assert!(!encoded.contains("endMs"));
    }

    #[test]
    fn invalid_words_degrade_to_the_complete_line() {
        for source in [
            WORDS.replace("text: world", "text: wrong"),
            WORDS.replace("end_ms: 1800", "end_ms: 500"),
            WORDS.replace("start_ms: 1800", "start_ms: 500"),
        ] {
            let document = parse(&source).unwrap();
            assert_eq!(document.lines[0].text, "Hello world");
            assert!(document.lines[0].words.is_empty());
        }
    }

    #[test]
    fn equal_starts_and_overlapping_vocals_are_preserved() {
        let source =
            format!("{WORDS}  - text: 'Other voice'\n    start_ms: 1000\n    end_ms: 2000\n");
        let document = parse(&source).unwrap();
        assert_eq!(document.lines.len(), 2);
        assert_eq!(document.lines[1].at_ms, 1000);
        let source = WORDS.replace("start_ms: 1800", "start_ms: 1500");
        assert_eq!(parse(&source).unwrap().lines[0].words.len(), 2);
    }

    #[test]
    fn line_only_plain_and_instrumental_files_are_supported() {
        let line = "version: '1.0'\nmetadata: {title: Test, artist: Test}\nlines: [{text: Line, start_ms: 1234}]";
        let document = parse(line).unwrap();
        assert!(document.synced);
        assert!(document.lines[0].words.is_empty());
        let plain = "version: '1.0'\nmetadata: {title: Test, artist: Test}\nlines: null\nplain: |\n  First\n  Second\n";
        let document = parse(plain).unwrap();
        assert!(!document.synced);
        assert_eq!(document.lines.len(), 2);
        let instrumental =
            "version: '1.0'\nmetadata: {title: Test, artist: Test, instrumental: true}";
        assert!(parse(instrumental).unwrap().lines[0]
            .text
            .contains("Instrumental"));
    }

    #[test]
    fn unsafe_or_unknown_yaml_is_refused() {
        for source in [
            WORDS.replace("'1.0'", "'2.0'"),
            WORDS.replace(
                "artist: Test artist",
                "artist: Test artist\n  offset_ms: 500",
            ),
            WORDS.replace("start_ms: 1000", "start_ms: -1"),
            format!("{WORDS}version: '1.0'\n"),
            WORDS.replace("title: Test song", "title: &title Test song"),
            WORDS.replace("title: Test song", "title: !custom Test song"),
            format!("{WORDS}---\n{{another: document}}"),
            format!("{WORDS}unknown: {}0{}", "[".repeat(20), "]".repeat(20)),
            "x".repeat(2 * 1024 * 1024 + 1),
        ] {
            assert!(parse(&source).is_none());
        }
    }
}
