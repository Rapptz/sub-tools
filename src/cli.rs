use clap::{
    builder::styling::{Effects, Reset, RgbColor, Style as AnsiStyle},
    Args, CommandFactory, Parser, Subcommand, ValueEnum,
};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    io::{stdin, stdout, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

use crate::{
    ass::{Ass, Colour},
    srt,
    utils::{windows_mut, LendingIterator},
    vtt, SubtitleFormat,
};

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

fn ass_tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r#"\{(.+)\}"#).unwrap())
}

fn allowed_ass_tags_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r#"(\\an\d)"#).unwrap())
}

fn drawing_events_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r#"\{\\p\d\}(.+)\{\\p\d\}"#).unwrap())
}

fn special_ass_character_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r#"\\(n|N|h)"#).unwrap())
}

fn clean_ass_text(s: &str) -> String {
    // Remove drawing events
    let result = drawing_events_regex().replace_all(s, "");
    // Remove all other ass tags
    let result = ass_tag_regex().replace_all(&result, |captures: &regex::Captures| {
        match allowed_ass_tags_regex().find(&captures[1]) {
            Some(m) => {
                let mut buffer = String::with_capacity(2 + m.len());
                buffer.push('{');
                buffer.push_str(m.as_str());
                buffer.push('}');
                buffer
            }
            None => String::new(),
        }
    });
    // Replace special characters
    special_ass_character_regex()
        .replace_all(&result, |captures: &regex::Captures| {
            match captures.get(1) {
                Some(m) => {
                    if m.as_str() == "N" {
                        "\n"
                    } else {
                        " "
                    }
                }
                None => " ", // This should technically be "unreachable"
            }
        })
        .into_owned()
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
    crate::utils::parse_duration(s).ok_or(InvalidDuration)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InputOutputLocation {
    Path(PathBuf),
    Stdio,
}

impl InputOutputLocation {
    fn new(path: PathBuf) -> Self {
        if path.as_os_str() == "-" {
            Self::Stdio
        } else {
            Self::Path(path)
        }
    }

    fn read_as_string(&self) -> std::io::Result<String> {
        match self {
            InputOutputLocation::Path(path) => crate::load_file(path),
            InputOutputLocation::Stdio => {
                let mut stdin = stdin();
                let mut buffer = String::new();
                stdin.read_to_string(&mut buffer)?;

                if buffer.starts_with('\u{feff}') {
                    // This is pretty inefficient but oh well
                    // U+FEFF is 3 bytes
                    buffer.drain(..3);
                }

                Ok(buffer)
            }
        }
    }

    fn save_ass(&self, ass: &Ass) -> anyhow::Result<()> {
        match self {
            InputOutputLocation::Path(path) => ass.save(path)?,
            InputOutputLocation::Stdio => ass.save_to_writer(stdout().lock())?,
        }
        Ok(())
    }

    fn save_srt(&self, dialogue: &[srt::Dialogue]) -> anyhow::Result<()> {
        match self {
            InputOutputLocation::Path(path) => srt::save(path, dialogue),
            InputOutputLocation::Stdio => {
                let buf = srt::save_to_string(dialogue);
                stdout().write_all(buf.as_bytes())?;
                Ok(())
            }
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Subcommands,
}

#[derive(Subcommand, Debug)]
pub enum Subcommands {
    /// Converts a subtitle from .vtt, .srt, or .ass to .srt or .ass
    Convert(ConvertArgs),
    /// Shifts a subtitle's dialogue by a given time
    Shift(ShiftArgs),
    /// Cleans up a subtitle file
    Cleanup(CleanupArgs),
    /// Shows some high level information about a subtitle file
    Info(InfoArgs),
}

#[derive(Debug, Copy, Clone, ValueEnum, PartialEq, Eq)]
pub enum ConvertFormat {
    Auto,
    Srt,
    Ass,
}

#[derive(Args, Default, Debug)]
#[group(required = false, multiple = true)]
pub struct DurationRange {
    /// The duration to start working from.
    ///
    /// When shifting or doing any type of editing work, start working
    /// on dialogue events that start at the specified duration.
    ///
    /// The duration format is `HH:MM:SS.ssss`. The `HH` and `.ssss`
    /// components are not required. For example, `10:24` and `00:10:24`
    /// are both accepted.
    #[arg(long, value_parser = parse_duration, verbatim_doc_comment)]
    pub start: Option<Duration>,
    /// The duration to finish working at.
    ///
    /// This is the upper end of the range similar to `--start`.
    #[arg(long, value_parser = parse_duration)]
    pub end: Option<Duration>,
}

impl DurationRange {
    pub fn contains(&self, duration: &Duration) -> bool {
        match (&self.start, &self.end) {
            (Some(start), Some(end)) => duration >= start && duration <= end,
            (Some(start), None) => duration >= start,
            (None, Some(end)) => duration <= end,
            (None, None) => true,
        }
    }
}

#[derive(Args, Default, Debug)]
#[group(required = false, multiple = false)]
pub struct InPlaceOutputArgs {
    /// Modify the subtitle in-place without creating another file
    #[arg(long)]
    pub in_place: bool,
    /// Where to output the file.
    ///
    /// If an output file is not provided and it is not an in-place edit,
    /// then it defaults to creating a file in the current working directory
    /// with the same filename as the input file but with `_modified`
    /// appended to the filename.
    ///
    /// If the output is being piped then it is printed into
    /// stdout instead.
    #[arg(short, long, verbatim_doc_comment)]
    pub output: Option<PathBuf>,
}

impl InPlaceOutputArgs {
    fn resolve(self, input: &Path) -> anyhow::Result<InputOutputLocation> {
        if let Some(output) = self.output {
            Ok(InputOutputLocation::Path(output))
        } else if self.in_place {
            Ok(InputOutputLocation::Path(input.to_path_buf()))
        } else if !stdout().is_terminal() || input.as_os_str() == "-" {
            Ok(InputOutputLocation::Stdio)
        } else {
            let mut path = PathBuf::new();
            match input.file_stem() {
                Some(filename) => {
                    let mut filename = filename.to_os_string();
                    filename.push("_modified");
                    if let Some(ext) = input.extension() {
                        filename.push(".");
                        filename.push(ext);
                    }
                    path.set_file_name(filename);
                    Ok(InputOutputLocation::Path(path))
                }
                None => anyhow::bail!("invalid filename given (no filename)"),
            }
        }
    }
}

#[derive(Args, Debug)]
pub struct ConvertArgs {
    #[arg(long, default_value_t = ConvertFormat::Auto, value_enum)]
    pub to: ConvertFormat,
    /// The subtitle file to convert to.
    ///
    /// If `-` is given, then it's interpreted as stdin.
    pub file: PathBuf,
    /// Where to output the file.
    ///
    /// The extension is used to figure out how to automatically
    /// detect the conversion format if provided. If an output
    /// file is not provided, then the conversion format
    /// must be given.
    ///
    /// If the program is being piped then it outputs
    /// the file to stdout.
    #[arg(short, long, verbatim_doc_comment)]
    pub output: Option<PathBuf>,
}

impl ConvertArgs {
    /// Returns the output filename.
    ///
    /// If the command line arguments are invalid then this exits.
    /// Otherwise this modifies `to` to the appropriate setting if
    /// set to `ConvertFormat::Auto`.
    fn validate_output(&mut self) -> InputOutputLocation {
        if self.to == ConvertFormat::Auto && self.output.is_none() {
            let mut cmd = Cli::command();
            cmd.error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "-o is required when using --to auto",
            )
            .exit();
        }

        if !matches!(
            self.file.extension().and_then(|s| s.to_str()),
            Some("ass" | "srt" | "vtt")
        ) {
            Cli::command()
                .error(
                    clap::error::ErrorKind::ValueValidation,
                    "input file must have .ass, .srt, or .vtt extension",
                )
                .exit()
        }

        match self.output.take() {
            Some(path) => {
                if self.to == ConvertFormat::Auto {
                    self.to = match path.extension().and_then(|s| s.to_str()) {
                        Some("ass") => ConvertFormat::Ass,
                        Some("srt") => ConvertFormat::Srt,
                        _ => Cli::command()
                            .error(
                                clap::error::ErrorKind::ValueValidation,
                                "could not determine output format for subtitle, use --to",
                            )
                            .exit(),
                    };
                }

                InputOutputLocation::Path(path)
            }
            None => {
                if !stdout().is_terminal() {
                    return InputOutputLocation::Stdio;
                }

                let extension = match self.to {
                    ConvertFormat::Ass => "ass",
                    ConvertFormat::Srt => "srt",
                    ConvertFormat::Auto => unreachable!(),
                };
                let mut output = PathBuf::new();
                if let Some(filename) = self.file.file_stem() {
                    let mut filename = filename.to_os_string();
                    filename.push(".");
                    filename.push(extension);
                    output.set_file_name(filename);
                    InputOutputLocation::Path(output)
                } else {
                    Cli::command()
                        .error(
                            clap::error::ErrorKind::ValueValidation,
                            "could not determine filename for input file",
                        )
                        .exit()
                }
            }
        }
    }

    /// Runs the conversion utility.
    pub fn run(mut self) -> anyhow::Result<()> {
        let output = self.validate_output();
        let input = InputOutputLocation::new(self.file);
        let contents = input.read_as_string()?;
        match SubtitleFormat::detect(&contents) {
            Some(SubtitleFormat::Ass) => {
                let ass = contents.parse::<Ass>()?;
                match self.to {
                    ConvertFormat::Srt => {
                        let dialogue = ass
                            .sections
                            .into_iter()
                            .filter_map(|s| s.try_into_events().ok())
                            .flat_map(|e| {
                                e.events
                                    .into_iter()
                                    .filter(|e| e.kind.is_dialogue())
                                    .enumerate()
                            })
                            .map(|(idx, e)| srt::Dialogue {
                                position: idx as u32 + 1,
                                start: e.start,
                                end: e.end,
                                text: clean_ass_text(&e.text),
                            })
                            .collect::<Vec<_>>();

                        output.save_srt(&dialogue)
                    }
                    ConvertFormat::Ass => {
                        // .ass -> .ass is a bit weird, but I guess
                        // just run it through the parser to clean it up
                        output.save_ass(&ass)
                    }
                    _ => Ok(()),
                }
            }
            Some(SubtitleFormat::Srt) => {
                let dialogue = srt::load_from_string(&contents)?;
                match self.to {
                    ConvertFormat::Srt => output.save_srt(&dialogue),
                    ConvertFormat::Ass => {
                        let ass = Ass::from_srt(dialogue);
                        output.save_ass(&ass)
                    }
                    _ => Ok(()),
                }
            }
            Some(SubtitleFormat::Vtt) => {
                let dialogue = vtt::load_from_string(&contents)?;
                match self.to {
                    ConvertFormat::Srt => output.save_srt(&dialogue),
                    ConvertFormat::Ass => {
                        let ass = Ass::from_srt(dialogue);
                        output.save_ass(&ass)
                    }
                    _ => Ok(()),
                }
            }
            _ => anyhow::bail!("Somehow got an invalid input file"),
        }
    }
}

#[derive(Args, Debug)]
pub struct InfoArgs {
    /// The subtitle file to get information for.
    ///
    /// If `-` is given, then it's interpreted as stdin.
    pub file: PathBuf,
}

struct ColourDisplay {
    name: &'static str,
    colour: Colour,
    spaced: bool,
}

impl ColourDisplay {
    fn new(name: &'static str, colour: Colour) -> Self {
        Self {
            name,
            colour,
            spaced: false,
        }
    }

    fn spaced(name: &'static str, colour: Colour) -> Self {
        Self {
            name,
            colour,
            spaced: true,
        }
    }

    fn proper_background(&self) -> RgbColor {
        // Contrast ratio is defined as L1 + 0.05 / L2 + 0.05
        // L2 is the relative luminance of the background colour
        // L1 is the relative luminance of the actual text colour
        // For simplicity the background colour can only be white/black
        let l1 = self.colour.relative_luminance();
        let l2 = Colour::BLACK.relative_luminance();
        let cr = (l1 + 0.05) / (l2 + 0.05);
        if cr >= 3.0 {
            RgbColor(0, 0, 0)
        } else {
            RgbColor(255, 255, 255)
        }
    }

    fn as_rgb(&self) -> RgbColor {
        RgbColor(self.colour.red, self.colour.green, self.colour.blue)
    }
}

impl std::fmt::Display for ColourDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}{}{}({})",
            self.as_rgb().render_fg(),
            self.proper_background().render_bg(),
            self.name,
            Reset,
            if self.spaced { "  " } else { " " },
            self.colour.to_hex()
        )
    }
}

impl InfoArgs {
    fn info_for_ass(&self, subs: Ass) {
        // Maybe at some point PlayResX/Y can be there too but
        // there's no point since like.. you can just see it in the file easily
        if let Some(section) = subs.sections.iter().find_map(|s| s.as_styles()) {
            println!("Styles:");
            for style in &section.styles {
                println!("  {}:", style.name);
                println!("    Font: {} at {}px", style.font_name, style.font_size);
                // Colors: #RRGGBBAA
                //   Primary (#aabbccdd)  Secondary  (#aabbccdd)
                //   Outline (#aabbccdd)  Background (#aabbccdd)
                println!("    Colors: #RRGGBBAA");
                let primary = ColourDisplay::new("Primary", style.primary_colour);
                println!(
                    "      {}  {}\n      {}  {}",
                    primary,
                    ColourDisplay::spaced("Secondary", style.secondary_colour),
                    ColourDisplay::new("Outline", style.outline_colour),
                    ColourDisplay::new("Background", style.background_colour)
                );
                let sample = AnsiStyle::new()
                    .effects(
                        Effects::new()
                            .set(Effects::BOLD, style.bold)
                            .set(Effects::ITALIC, style.italic)
                            .set(Effects::UNDERLINE, style.underline)
                            .set(Effects::STRIKETHROUGH, style.striked),
                    )
                    .fg_color(Some(primary.as_rgb().into()));
                println!("    Sample Text: {sample}Hello, こんにちは{sample:#}");
                //  Text Properties:
                //    Scale: (100%, 100%)  Spacing: 0px  Angle: 0.0
                //  Border Style: 4px Outline [with 4px drop shadow]
                //  Alignment: Top Left
                //  Margin: L: 10, R: 10, V: 20
                //  Encoding: 1
                println!("    Text Properties:");
                println!(
                    "      Scale: ({}%, {}%)  Spacing: {}px  Angle: {}°",
                    style.scale_x, style.scale_y, style.spacing, style.angle
                );
                if style.border_style == 1 {
                    print!("    Border Style: {}px Outline", style.outline);
                    if style.shadow != 0.0 {
                        println!(" with {}px drop shadow", style.shadow);
                    } else {
                        println!();
                    }
                } else if style.border_style == 3 {
                    println!("    Border Style: Opaque Box");
                } else {
                    println!("    Border Style: Unknown ({})", style.border_style);
                }

                let alignment = match style.alignment {
                    1 => "Bottom Left",
                    2 => "Bottom Middle",
                    3 => "Bottom Right",
                    4 => "Center Left",
                    5 => "Center",
                    6 => "Center Right",
                    7 => "Top Left",
                    8 => "Top Center",
                    9 => "Top Right",
                    _ => "Unknown",
                };
                println!("    Alignment: {alignment}");
                println!(
                    "    Margin: L: {}px, R: {}px, V: {}px",
                    style.margin_l, style.margin_r, style.margin_v
                );
                println!("    Encoding: {}", style.encoding);
            }

            println!();
        }

        let mut counter = HashMap::new();
        for event in subs.events().filter(|s| s.kind.is_dialogue()) {
            counter
                .entry(event.style.as_str())
                .and_modify(|x| *x += 1)
                .or_insert(1);
        }
        println!("Dialogue:");
        for (style, count) in counter.iter() {
            println!("  {style}: {count}");
        }
        let sum = counter.values().sum::<i32>();
        println!("  Total: {sum}");
    }

    fn simple_info(&self, dialogue: &[srt::Dialogue]) {
        println!("Dialogue:\n  Total: {}", dialogue.len())
    }

    pub fn run(self) -> anyhow::Result<()> {
        let input = InputOutputLocation::new(self.file.clone());
        let contents = input.read_as_string()?;
        let format = SubtitleFormat::detect(&contents);
        match format {
            Some(SubtitleFormat::Ass) => {
                let subs = contents.parse()?;
                self.info_for_ass(subs);
                Ok(())
            }
            Some(SubtitleFormat::Vtt) => {
                let dialogue = vtt::load_from_string(&contents)?;
                self.simple_info(&dialogue);
                Ok(())
            }
            Some(SubtitleFormat::Srt) => {
                let dialogue = srt::load_from_string(&contents)?;
                self.simple_info(&dialogue);
                Ok(())
            }
            _ => Cli::command()
                .error(
                    clap::error::ErrorKind::ValueValidation,
                    "could not recognize subtitle type",
                )
                .exit(),
        }
    }
}

#[derive(Args, Debug)]
pub struct ShiftArgs {
    /// The subtitle file to shift
    ///
    /// If `-` is given, then it's interpreted as stdin.
    file: PathBuf,
    #[command(flatten)]
    output: InPlaceOutputArgs,
    #[command(flatten)]
    range: DurationRange,
    /// Shift the timing of the subtitles by the given seconds
    #[arg(long, required = true, value_parser = valid_duration, allow_negative_numbers = true)]
    by: f32,
}

impl ShiftArgs {
    pub fn run(self) -> anyhow::Result<()> {
        let output = self.output.resolve(&self.file)?;
        let input = InputOutputLocation::new(self.file);
        let contents = input.read_as_string()?;
        match SubtitleFormat::detect(&contents) {
            Some(SubtitleFormat::Ass) => {
                let mut subs = contents.parse::<Ass>()?;
                subs.events_mut()
                    .filter(|e| self.range.contains(&e.start))
                    .for_each(|e| e.shift_by(self.by));
                output.save_ass(&subs)
            }
            Some(SubtitleFormat::Srt) => {
                let mut dialogue = srt::load_from_string(&contents)?;
                dialogue
                    .iter_mut()
                    .filter(|d| self.range.contains(&d.start))
                    .for_each(|d| d.shift_by(self.by));
                output.save_srt(&dialogue)
            }
            Some(_) => Cli::command()
                .error(
                    clap::error::ErrorKind::ValueValidation,
                    "unsupported subtitle format for this operation",
                )
                .exit(),
            None => Cli::command()
                .error(
                    clap::error::ErrorKind::ValueValidation,
                    "could not recognize subtitle type",
                )
                .exit(),
        }
    }
}

#[derive(Args, Debug)]
pub struct CleanupArgs {
    /// The subtitle file to cleanup
    ///
    /// If `-` is given, then it's interpreted as stdin.
    file: PathBuf,
    #[command(flatten)]
    output: InPlaceOutputArgs,
    #[command(flatten)]
    range: DurationRange,
    /// Remove comment lines from the file (.ass only).
    #[arg(long)]
    comments: bool,
    /// Remove unused styles from the file (.ass only).
    #[arg(long)]
    unused_styles: bool,
    /// Fixes common issues with Japanese subtitle files.
    ///
    /// The things removed are as follows:
    ///
    /// * [外:37F6ECF37A0A3EF8DFF083CCC8754F81]-like instances of text
    /// * Half-width kana is converted into full width kana
    /// * Removal of &lrm;, U+202A, and U+202C characters
    #[arg(long = "fix-jp", verbatim_doc_comment)]
    fix_japanese: bool,
    /// Remove all dialogue lines from the file.
    ///
    /// This is only useful if specified with a start and
    /// end range.
    #[arg(long)]
    remove: bool,
    /// Removes dialogue using the given style (.ass only)
    ///
    /// Can be specified multiple times to remove multiple
    /// dialogues from different styles.
    #[arg(long, verbatim_doc_comment)]
    dialogue_from: Vec<String>,
    /// Merges simultaneous dialogue lines that have the same start and end time.
    ///
    /// This is a common trick used in some .ass files. Merging is done by
    /// combining the dialogue top to bottom with a new line between each.
    #[arg(long, verbatim_doc_comment)]
    merge_simultaneous: bool,
}

impl CleanupArgs {
    pub fn run(self) -> anyhow::Result<()> {
        let output = self.output.resolve(&self.file)?;
        let input = InputOutputLocation::new(self.file);
        let contents = input.read_as_string()?;
        match SubtitleFormat::detect(&contents) {
            Some(SubtitleFormat::Srt) => {
                let mut dialogue = srt::load_from_string(&contents)?;
                if self.remove {
                    dialogue.retain(|d| !self.range.contains(&d.start));
                }
                if self.merge_simultaneous {
                    let mut windows = windows_mut(&mut dialogue);
                    while let Some([left, right]) = windows.next() {
                        if left.start == right.start && left.end == right.end {
                            left.text.push('\n');
                            left.text.push_str(&right.text);
                            right.position = u32::MAX; // sentinel to mark for deletion
                        }
                    }
                    dialogue.retain(|d| d.position != u32::MAX);
                }
                if self.fix_japanese {
                    dialogue
                        .iter_mut()
                        .filter(|d| self.range.contains(&d.start))
                        .for_each(|d| crate::japanese::fix_broken_text(&mut d.text));
                }

                // Fix up the SRT position markers
                for (index, d) in dialogue.iter_mut().enumerate() {
                    d.position = (index + 1) as u32;
                }

                output.save_srt(&dialogue)
            }
            Some(SubtitleFormat::Ass) => {
                let mut subs = contents.parse::<Ass>()?;

                // This removes *all* comments from the file
                if self.comments {
                    for section in &mut subs.sections {
                        section.remove_comments();
                    }
                }

                if let Some(section) = subs.sections.iter_mut().find_map(|s| s.as_events_mut()) {
                    let mut used_styles = HashSet::new();
                    if self.remove {
                        section.events.retain(|e| !self.range.contains(&e.start));
                    }

                    // Do this in two passes to keep track of used styles
                    let removed_styles = self.dialogue_from.into_iter().collect::<HashSet<_>>();
                    for event in &mut section.events {
                        if !used_styles.contains(event.style.as_str()) {
                            used_styles.insert(event.style.clone());
                        }
                        if removed_styles.contains(&event.style) {
                            event.start = Duration::MAX; // sentinel
                        }

                        if self.fix_japanese && event.kind.is_dialogue() {
                            crate::japanese::fix_broken_text(&mut event.text);
                        }
                    }

                    if self.merge_simultaneous {
                        let mut windows = windows_mut(&mut section.events);
                        while let Some([left, right]) = windows.next() {
                            if left.start != Duration::MAX
                                && left.start == right.start
                                && left.end == right.end
                            {
                                left.text.push_str("\\N");
                                left.text.push_str(&right.text);
                                right.start = Duration::MAX; // sentinel to mark for deletion
                            }
                        }
                    }

                    #[allow(clippy::nonminimal_bool)]
                    section.events.retain(|d| {
                        d.start != Duration::MAX
                            && !(self.unused_styles && !used_styles.contains(d.style.as_str()))
                    });
                }
                output.save_ass(&subs)
            }
            Some(_) => Cli::command()
                .error(
                    clap::error::ErrorKind::ValueValidation,
                    "unsupported subtitle format for this operation",
                )
                .exit(),
            None => Cli::command()
                .error(
                    clap::error::ErrorKind::ValueValidation,
                    "could not recognize subtitle type",
                )
                .exit(),
        }
    }
}
