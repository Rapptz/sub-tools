use regex::Regex;

use crate::srt::{parse_srt_time, Dialogue};
use std::{path::Path, sync::OnceLock};

fn cue_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"(?x)
        (?P<start>(?:\d{2}:)?\d{2}:\d{2}[\.,]\d{3})
        \s-->\s
        (?P<end>(?:\d{2}:)?\d{2}:\d{2}[\.,]\d{3})
        (?:.*(line:(?P<line>[0-9.]+?))%)?"#,
        )
        .unwrap()
    })
}

fn text_cleanup_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r#"(</?c\.[a-zA-Z_\s]+>|&lrm;|&rlm;)"#).unwrap())
}

fn parse_dialogue(segment: &str) -> Option<Dialogue> {
    let mut lines = segment.splitn(3, '\n');
    let position: u32 = lines.next().and_then(|s| s.parse().ok())?;
    let cue = cue_regex().captures(lines.next()?)?;
    let start = parse_srt_time(&cue["start"])?;
    let end = parse_srt_time(&cue["end"])?;
    let top = cue
        .name("line")
        .and_then(|s| s.as_str().parse::<f32>().ok())
        .map(|f| f < 50.0)
        .unwrap_or_default();

    let mut text = text_cleanup_regex()
        .replace_all(lines.next()?, "")
        .into_owned();

    if top {
        text.insert_str(0, "{\\an8}")
    }

    Some(Dialogue {
        position,
        start,
        end,
        text,
    })
}

pub fn load(path: &Path) -> std::io::Result<Vec<Dialogue>> {
    let buffer = crate::load_file(path)?;
    load_from_string(&buffer)
}

pub fn load_from_string(buffer: &str) -> std::io::Result<Vec<Dialogue>> {
    let Some(index) = buffer.find("\n1\n") else {
        return Err(std::io::Error::other("no dialogue found"));
    };

    Ok(buffer[index - 1..]
        .split_terminator("\n\n")
        .flat_map(parse_dialogue)
        .collect::<Vec<_>>())
}
