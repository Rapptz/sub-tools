use std::{error::Error, fmt::Display, str::FromStr, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialogue {
    pub position: u32,
    pub start: Duration,
    pub end: Duration,
    pub text: String,
}

#[inline]
fn is_halfwidth_kana(c: char) -> bool {
    ('\u{FF65}'..='\u{FF9F}').contains(&c)
}

#[inline]
fn is_katakana(c: char) -> bool {
    ('\u{30A0}'..='\u{30FA}').contains(&c)
}

fn replace_halfwith_kana(input: &str) -> Option<String> {
    let index = input.find(is_halfwidth_kana)?;
    let mut output = String::from(&input[..index]);
    output.reserve(input.len() - index);
    let chars = input[index..].chars();
    for ch in chars {
        match ch {
            '･' => output.push('・'),
            'ｦ' => output.push('ヲ'),
            'ｧ' => output.push('ァ'),
            'ｨ' => output.push('ィ'),
            'ｩ' => output.push('ゥ'),
            'ｪ' => output.push('ェ'),
            'ｫ' => output.push('ォ'),
            'ｬ' => output.push('ャ'),
            'ｭ' => output.push('ュ'),
            'ｮ' => output.push('ョ'),
            'ｯ' => output.push('ッ'),
            'ｰ' => output.push('ー'),
            'ｱ' => output.push('ア'),
            'ｲ' => output.push('イ'),
            'ｳ' => output.push('ウ'),
            'ｴ' => output.push('エ'),
            'ｵ' => output.push('オ'),
            'ｶ' => output.push('カ'),
            'ｷ' => output.push('キ'),
            'ｸ' => output.push('ク'),
            'ｹ' => output.push('ケ'),
            'ｺ' => output.push('コ'),
            'ｻ' => output.push('サ'),
            'ｼ' => output.push('シ'),
            'ｽ' => output.push('ス'),
            'ｾ' => output.push('セ'),
            'ｿ' => output.push('ソ'),
            'ﾀ' => output.push('タ'),
            'ﾁ' => output.push('チ'),
            'ﾂ' => output.push('ツ'),
            'ﾃ' => output.push('テ'),
            'ﾄ' => output.push('ト'),
            'ﾅ' => output.push('ナ'),
            'ﾆ' => output.push('ニ'),
            'ﾇ' => output.push('ヌ'),
            'ﾈ' => output.push('ネ'),
            'ﾉ' => output.push('ノ'),
            'ﾊ' => output.push('ハ'),
            'ﾋ' => output.push('ヒ'),
            'ﾌ' => output.push('フ'),
            'ﾍ' => output.push('ヘ'),
            'ﾎ' => output.push('ホ'),
            'ﾏ' => output.push('マ'),
            'ﾐ' => output.push('ミ'),
            'ﾑ' => output.push('ム'),
            'ﾒ' => output.push('メ'),
            'ﾓ' => output.push('モ'),
            'ﾔ' => output.push('ヤ'),
            'ﾕ' => output.push('ユ'),
            'ﾖ' => output.push('ヨ'),
            'ﾗ' => output.push('ラ'),
            'ﾘ' => output.push('リ'),
            'ﾙ' => output.push('ル'),
            'ﾚ' => output.push('レ'),
            'ﾛ' => output.push('ロ'),
            'ﾜ' => output.push('ワ'),
            'ﾝ' => output.push('ン'),
            // These two are a bit of a special case
            // They might cause some mess ups, but ideally it doesn't
            'ﾞ' => {
                // In the katakana table, a voiced dakuten
                // is merely +1 char from the previous one
                match output.pop() {
                    Some(ch) => {
                        if is_katakana(ch) {
                            output.push(char::from_u32(ch as u32 + 1).unwrap());
                        } else {
                            output.push(ch);
                            output.push('゛');
                        }
                    }
                    None => output.push('゛'),
                }
            }
            'ﾟ' => {
                // In the katakana table, a voiced dakuten
                // is merely +2 char from the previous one
                match output.pop() {
                    Some(ch) => {
                        if is_katakana(ch) {
                            output.push(char::from_u32(ch as u32 + 2).unwrap());
                        } else {
                            output.push(ch);
                            output.push('゜');
                        }
                    }
                    None => output.push('゜'),
                }
            }
            _ => output.push(ch),
        }
    }
    Some(output)
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

    pub fn fix_japanese(&mut self) {
        // Remove e.g. [外:37F6ECF37A0A3EF8DFF083CCC8754F81]-like instances of text
        if let Some(index) = self.text.find("[外:") {
            let sub = &self.text[index..];
            // Find the first instance that's not uppercase hex
            if let Some(end) = sub.find(|c: char| !c.is_ascii_hexdigit()) {
                if sub.as_bytes()[end] == b']' {
                    self.text.drain(index..=end);
                }
            }
        }

        // Fix up half-width kana
        if let Some(result) = replace_halfwith_kana(&self.text) {
            self.text = result;
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
                "{:02}:{:02}:{:02},{}",
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

fn parse_srt_time(s: &str) -> Option<Duration> {
    // HH:MM:SS,mmm
    let (rest, ms) = s.split_once(',')?;
    let mut split = rest.trim().splitn(3, ':');
    let hours: u64 = split.next()?.parse().ok()?;
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
