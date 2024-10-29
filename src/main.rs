use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use clap::Parser;
use sub_tools::srt::Dialogue;

/// A duration that can be parsed from the command line or as a string input.
///
/// The format is `HH:MM:SS.ssss` with `HH` and `.ssss` being optional.
/// So e.g. 10:24 is 10 minutes and 24 seconds but 10:24:00 is 10 hours, 24 minutes and 0 seconds.
fn parse_duration_helper(s: &str) -> Option<Duration> {
    let mut components = s.splitn(3, ':').map(|s| s.parse::<u64>().ok());
    let first = components.next()??;
    let second = components.next()??;
    match components.next() {
        Some(Some(third)) => {
            // This case contains hours, minutes, and seconds
            Some(Duration::from_secs(first * 3600 + second * 60 + third))
        }
        Some(None) => {
            // This one's an invalid parse, e.g. 10:24:aa
            None
        }
        None => {
            // This case is just 10:24 or 10m24s
            Some(Duration::from_secs(first * 60 + second))
        }
    }
}

fn parse_duration_fractional_helper(s: &str) -> Option<Duration> {
    match s.split_once('.') {
        Some((duration, fractional)) => {
            let duration = parse_duration_helper(duration)?;
            let ms = Duration::from_millis(fractional.parse().ok()?);
            Some(duration + ms)
        }
        None => parse_duration_helper(s),
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct InvalidDuration;

impl std::fmt::Display for InvalidDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid duration given (must be HH:MM:SS.ssss format, HH is optional)")
    }
}

impl std::error::Error for InvalidDuration {}

fn parse_duration(s: &str) -> Result<Duration, InvalidDuration> {
    parse_duration_fractional_helper(s).ok_or(InvalidDuration)
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The subtitle files to operate on
    ///
    /// If a vtt is given, then it is converted into an srt
    files: Vec<PathBuf>,
    /// Shift the times of the subtitles by the given seconds
    #[arg(long, value_parser = valid_duration, allow_negative_numbers = true)]
    shift: Option<f32>,
    /// Fixes common issues with Japanese subtitle files.
    ///
    /// The things removed are as follows:
    ///
    /// * [外:37F6ECF37A0A3EF8DFF083CCC8754F81]-like instances of text
    /// * Half-width kana is converted into full width kana
    /// * Removal of &lrm;, U+202A, and U+202C characters
    #[arg(
        long = "fix-jp",
        required = false,
        default_value_t = false,
        verbatim_doc_comment
    )]
    fix_japanese: bool,

    /// The duration to start working from.
    ///
    /// When shifting or doing any type of editing work, start working
    /// on dialogue events that start at the specified duration.
    ///
    /// The duration format is `HH:MM:SS.ssss`. The `HH` and `.ssss`
    /// components are not required. For example, `10:24` and `00:10:24`
    /// are both accepted.
    #[arg(long, value_parser = parse_duration, verbatim_doc_comment)]
    start: Option<Duration>,
    /// The duration to finish working at.
    ///
    /// This is the upper end of the range similar to `--start`.
    #[arg(long, value_parser = parse_duration)]
    end: Option<Duration>,
}

impl Args {
    fn is_within_duration(&self, duration: &Duration) -> bool {
        match (&self.start, &self.end) {
            (Some(start), Some(end)) => duration >= start && duration <= end,
            (Some(start), None) => duration >= start,
            (None, Some(end)) => duration <= end,
            (None, None) => true,
        }
    }
}

fn valid_duration(s: &str) -> Result<f32, String> {
    let time: f32 = s
        .parse()
        .map_err(|_| format!("`{s}` isn't a valid duration"))?;
    if !time.is_finite() || time == -0.0 {
        Err(format!("`{s}` isn't a valid duration"))
    } else {
        Ok(time)
    }
}

#[derive(Debug)]
struct InProgressFile {
    pending_filename: PathBuf,
    dialogue: Vec<Dialogue>,
    dirty: bool,
}

impl InProgressFile {
    fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let is_vtt = path.extension().map(|s| s == "vtt").unwrap_or_default();
        let new_filename = match path.file_stem() {
            Some(filename) => {
                let mut new_file = path.to_path_buf();
                let mut filename = filename.to_os_string();
                if is_vtt {
                    filename.push(".srt")
                } else {
                    filename.push("_modified.srt");
                }
                new_file.set_file_name(filename);
                new_file
            }
            None => anyhow::bail!("invalid filename given"),
        };

        let dialogue = if is_vtt {
            sub_tools::vtt::load(path)?
        } else {
            sub_tools::srt::load(path)?
        };

        Ok(Self {
            pending_filename: new_filename,
            dialogue,
            dirty: is_vtt,
        })
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    const fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn save(&self) -> anyhow::Result<()> {
        let mut new_contents = self
            .dialogue
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join("\n\n");

        new_contents.push_str("\n\n");
        let mut new_fp = File::create(&self.pending_filename)
            .with_context(|| "could not create new subtitle file".to_string())?;
        new_fp.write_all(new_contents.as_bytes())?;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let files: anyhow::Result<Vec<_>> = args.files.iter().map(InProgressFile::new).collect();
    let mut files = files?;
    if let Some(shift) = args.shift {
        for file in files.iter_mut() {
            file.mark_dirty();
            for dialogue in file.dialogue.iter_mut() {
                if args.is_within_duration(&dialogue.start) {
                    dialogue.shift_by(shift);
                }
            }
        }
    }

    if args.fix_japanese {
        for file in files.iter_mut() {
            file.mark_dirty();
            for dialogue in file.dialogue.iter_mut() {
                if args.is_within_duration(&dialogue.start) {
                    dialogue.fix_japanese();
                }
            }
        }
    }

    for file in files {
        if file.is_dirty() {
            file.save()?;
        }
    }

    Ok(())
}
