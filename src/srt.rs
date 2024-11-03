use std::{error::Error, fmt::Display, io::Write, path::Path, str::FromStr, time::Duration};

use anyhow::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialogue {
    pub position: u32,
    pub start: Duration,
    pub end: Duration,
    pub text: String,
}

impl Dialogue {
    pub fn shift_by(&mut self, seconds: f32) {
        let duration = Duration::from_secs_f32(seconds.abs());
        if seconds < 0.0 {
            self.start = self.start.saturating_sub(duration);
            self.end = self.end.saturating_sub(duration);
        } else {
            self.start = self.start.saturating_add(duration);
            self.end = self.end.saturating_add(duration);
        }
    }
}

impl Display for Dialogue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn duration_to_srt(f: &mut std::fmt::Formatter<'_>, d: &Duration) -> std::fmt::Result {
            let seconds = d.as_secs();
            let (hours, seconds) = (seconds / 3600, seconds % 3600);
            let (minutes, seconds) = (seconds / 60, seconds % 60);
            let ms = d.subsec_millis();
            f.write_fmt(format_args!(
                "{:02}:{:02}:{:02},{:03}",
                hours, minutes, seconds, ms
            ))
        }

        self.position.fmt(f)?;
        f.write_str("\n")?;
        duration_to_srt(f, &self.start)?;
        f.write_str(" --> ")?;
        duration_to_srt(f, &self.end)?;
        f.write_str("\n")?;
        f.write_str(&self.text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseDialogueError {
    Position,
    Start,
    End,
    Separator,
    EmptyDialogue,
}

impl Display for ParseDialogueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseDialogueError::Position => {
                f.write_str("could not parse srt dialogue: bad position")
            }
            ParseDialogueError::Start => f.write_str("could not parse srt dialogue: bad start"),
            ParseDialogueError::End => f.write_str("could not parse srt dialogue: bad end"),
            ParseDialogueError::Separator => {
                f.write_str("could not parse srt dialogue: bad or missing separator")
            }
            ParseDialogueError::EmptyDialogue => {
                f.write_str("could not parse srt dialogue: no dialogue")
            }
        }
    }
}

pub(crate) fn parse_srt_time(s: &str) -> Option<Duration> {
    // HH:MM:SS,mmm
    // HH is optional (due to VTT)
    let (rest, ms) = s.split_once([',', '.'])?;
    let mut split = rest.trim().splitn(3, ':');
    let hours: u64 = split.next()?.parse().ok().unwrap_or_default();
    let minutes: u64 = split.next()?.parse().ok()?;
    let seconds: u64 = split.next()?.parse().ok()?;
    let seconds = seconds + (minutes * 60) + (hours * 3600);
    let nanos = ms.trim().parse::<u32>().ok()?.saturating_mul(1_000_000);
    Some(Duration::new(seconds, nanos))
}

impl Error for ParseDialogueError {}

impl FromStr for Dialogue {
    type Err = ParseDialogueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut lines = s.splitn(3, '\n');
        let position: u32 = lines
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or(ParseDialogueError::Position)?;
        let (start, end) = match lines.next() {
            Some(times) => {
                let (start, end) = times
                    .split_once(" --> ")
                    .ok_or(ParseDialogueError::Separator)?;
                let start = parse_srt_time(start).ok_or(ParseDialogueError::Start)?;
                let end = parse_srt_time(end).ok_or(ParseDialogueError::End)?;
                (start, end)
            }
            None => return Err(ParseDialogueError::Start),
        };
        let text = lines
            .next()
            .ok_or(ParseDialogueError::EmptyDialogue)?
            .to_owned();
        Ok(Self {
            position,
            start,
            end,
            text,
        })
    }
}

pub fn load(path: &Path) -> anyhow::Result<Vec<Dialogue>> {
    use anyhow::Context;
    let buffer = crate::load_file(path)?;
    buffer
        .split_terminator("\n\n")
        .enumerate()
        .map(|(i, s)| {
            s.parse::<Dialogue>()
                .with_context(|| format!("from srt dialogue {}", i + 1))
        })
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to extract dialogue from {}", path.display()))
}

pub fn save(path: &Path, dialogue: Vec<Dialogue>) -> anyhow::Result<()> {
    let mut new_contents = dialogue
        .into_iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join("\n\n");

    new_contents.push_str("\n\n");
    let mut new_fp = std::fs::File::create(path)
        .with_context(|| "could not create new subtitle file".to_string())?;
    new_fp.write_all(new_contents.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialogue() {
        let fragment =
            "11\n00:00:22,814 --> 00:00:26,609\nもう ４月というのに\n何やら 今日は冷えますね";
        let result = fragment.parse::<Dialogue>().expect("could not parse");
        assert_eq!(result.position, 11);
        assert_eq!(result.start.as_secs(), 22);
        assert_eq!(result.start.subsec_millis(), 814);
        assert_eq!(result.end.as_secs(), 26);
        assert_eq!(result.end.subsec_millis(), 609);
        assert_eq!(result.text, "もう ４月というのに\n何やら 今日は冷えますね");

        assert_eq!(result.to_string(), fragment);
    }
}
