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

fn parse_dialogue((default_position, segment): (usize, &str)) -> Option<Dialogue> {
    let mut lines = crate::utils::Lines::new(segment);
    let dialogue_or_position = lines.next()?;
    let (position, cue) = match dialogue_or_position.parse::<u32>() {
        Ok(position) => (position, cue_regex().captures(lines.next()?)?),
        Err(_) => (
            default_position as u32,
            cue_regex().captures(dialogue_or_position)?,
        ),
    };
    let start = parse_srt_time(&cue["start"])?;
    let end = parse_srt_time(&cue["end"])?;
    let top = cue
        .name("line")
        .and_then(|s| s.as_str().parse::<f32>().ok())
        .map(|f| f < 50.0)
        .unwrap_or_default();

    let mut text = text_cleanup_regex()
        .replace_all(lines.remainder(), "")
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

/// This finds the backup dialogue option when there are no position markers
///
/// This walks back based off of the --> sentinel so it's a little slow
fn find_backup_dialogue(s: &str) -> Option<usize> {
    let first = s.find("-->")?;
    s[..first].rfind('\n')
}

pub fn load_from_string(buffer: &str) -> std::io::Result<Vec<Dialogue>> {
    if !buffer.starts_with("WEBVTT\n") {
        return Err(std::io::Error::other("invalid vtt file (missing header)"));
    }

    let Some(index) = buffer
        .find("\n1\n")
        .or_else(|| find_backup_dialogue(buffer))
    else {
        return Err(std::io::Error::other("no dialogue found"));
    };

    Ok(buffer[index - 1..]
        .trim_end()
        .split_terminator("\n\n")
        .enumerate()
        .flat_map(parse_dialogue)
        .collect::<Vec<_>>())
}
