use std::{
    fs::File,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::Parser;
use sub_tools::srt::Dialogue;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The subtitle files to operate on
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

impl Args {
    fn is_modified(&self) -> bool {
        self.fix_japanese || self.shift.is_some()
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
}

impl InProgressFile {
    fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let mut fp = File::open(path)?;
        let new_filename = match path.file_stem() {
            Some(filename) => {
                let mut new_file = PathBuf::new();
                let mut filename = filename.to_os_string();
                filename.push("_modified.srt");
                new_file.set_file_name(filename);
                new_file
            }
            None => anyhow::bail!("invalid filename given"),
        };

        let mut buffer = String::new();
        // Try to check if there's a UTF-8 BOM somewhere
        let mut bom: [u8; 3] = [0; 3];
        fp.read_exact(&mut bom)?;
        if bom != [0xEF, 0xBB, 0xBF] {
            fp.rewind()?;
        }

        fp.read_to_string(&mut buffer)?;

        if buffer.contains("\r\n") {
            buffer = buffer.replace("\r\n", "\n");
        }
        let dialogue = buffer
            .split_terminator("\n\n")
            .enumerate()
            .map(|(i, s)| {
                s.parse::<Dialogue>()
                    .with_context(|| format!("from srt dialogue {}", i + 1))
            })
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("Failed to extract dialogue from {}", path.display()))?;

        Ok(Self {
            pending_filename: new_filename,
            dialogue,
        })
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
            for dialogue in file.dialogue.iter_mut() {
                dialogue.shift_by(shift);
            }
        }
    }

    if args.fix_japanese {
        for file in files.iter_mut() {
            for dialogue in file.dialogue.iter_mut() {
                dialogue.fix_japanese();
            }
        }
    }
    if !files.is_empty() && args.is_modified() {
        for file in files {
            file.save()?;
        }
    }
    Ok(())
}
