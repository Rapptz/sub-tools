use std::{borrow::Cow, time::Duration};

pub(crate) trait LendingIterator {
    type Item<'this>
    where
        Self: 'this;
    fn next(&mut self) -> Option<Self::Item<'_>>;
}

pub(crate) struct WindowsMut<'a, T, const SIZE: usize> {
    slice: &'a mut [T],
    start: usize,
}

impl<'a, T, const SIZE: usize> LendingIterator for WindowsMut<'a, T, SIZE> {
    type Item<'this>
        = &'this mut [T; SIZE]
    where
        'a: 'this;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        let result = self
            .slice
            .get_mut(self.start..)?
            .get_mut(..SIZE)?
            .try_into()
            .unwrap();
        self.start += 1;
        Some(result)
    }
}

pub(crate) fn windows_mut<T, const SIZE: usize>(slice: &mut [T]) -> WindowsMut<'_, T, SIZE> {
    assert_ne!(SIZE, 0);
    WindowsMut { slice, start: 0 }
}

fn maybe_replace_separator(s: &str) -> Cow<'_, str> {
    match s.find(',') {
        None => Cow::Borrowed(s),
        Some(idx) => {
            let mut output = String::from(&s[..idx]);
            output.reserve(s.len() - idx);
            output.push('.');
            output.push_str(&s[idx + 1..]);
            Cow::Owned(output)
        }
    }
}

/// A duration that can be parsed from the command line or as a string input.
///
/// The format is `HH:MM:SS.ssss` with `HH` and `.ssss` being optional.
/// So e.g. 10:24 is 10 minutes and 24 seconds but 10:24:00 is 10 hours, 24 minutes and 0 seconds.
pub(crate) fn parse_duration(s: &str) -> Option<Duration> {
    let mut components = s.splitn(3, ':');
    let first = components.next()?;
    let second = components.next()?;
    match components.next() {
        Some(third) => {
            // This case contains hours, minutes, and seconds
            let hours: f64 = first.parse().ok()?;
            let minutes: f64 = second.parse().ok()?;
            let seconds: f64 = maybe_replace_separator(third).parse().ok()?;
            Some(Duration::from_secs_f64(
                hours * 3600.0 + minutes * 60.0 + seconds,
            ))
        }
        None => {
            let minutes: f64 = first.parse().ok()?;
            let seconds: f64 = maybe_replace_separator(second).parse().ok()?;
            // This case is just 10:24 or 10m24s
            Some(Duration::from_secs_f64(minutes * 60.0 + seconds))
        }
    }
}

/// An iterator for lines similar to Lines but has an exposed `remainder` method
/// in stable Rust
pub(crate) struct Lines<'a>(&'a str);

impl Lines<'_> {
    pub(crate) fn new(s: &str) -> Lines<'_> {
        Lines(s)
    }

    pub(crate) const fn remainder(&self) -> &str {
        self.0
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let (segment, remainder) = self.0.split_once('\n')?;
        self.0 = remainder;
        Some(segment)
    }
}
