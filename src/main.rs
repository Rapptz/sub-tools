use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::Parser;
use sub_tools::srt::Dialogue;

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
    ///
    /// * Half-width kana is converted into full width kana
    ///
    /// * Removal of &lrm;, U+202A, and U+202C characters
    #[arg(long = "fix-jp", required = false, default_value_t = false)]
    fix_japanese: bool,
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
                let mut new_file = PathBuf::new();
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
                dialogue.shift_by(shift);
            }
        }
    }

    if args.fix_japanese {
        for file in files.iter_mut() {
            file.mark_dirty();
            for dialogue in file.dialogue.iter_mut() {
                dialogue.fix_japanese();
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
