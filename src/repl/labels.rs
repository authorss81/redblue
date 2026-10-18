//! Where `:session` records the name a session was given, and what `:sessions`
//! reads back out of it.
//!
//! A session name is not decoration: a debugging session that took twenty
//! minutes is worth nothing if next week nobody can tell which of yesterday's
//! sessions it was. `:save` and `:restore` already let a session be kept — under
//! a file name the user chose — and this is the other half: a name the session
//! carries, in a listing they can browse.
//!
//! The file is append-only, one record per line, `<unix-seconds>\t<name>`, and
//! the listing is derived from it rather than kept beside it: a listing kept in
//! the file would have to be rewritten by every session, and two sessions
//! writing it at once would lose one of the names.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// How long a name stays in the listing: a week, so `:sessions` is a question
/// about the work in hand rather than a history of every session ever run.
pub const WEEK_SECONDS: u64 = 7 * 24 * 60 * 60;

/// The file the records are kept in, under the user's home directory.
pub const LABEL_FILE: &str = "sessions";

/// The longest name [`record`] will write.
///
/// A cap rather than a truncation: a name silently cut in half is a name the
/// user did not type, and the listing would then hold two sessions that look
/// alike and are not.
pub const MAX_NAME_BYTES: usize = 120;

/// The most bytes of records [`used`] will read, and [`record`] will append to.
///
/// Bounded because the file grows by one line per named session and nothing
/// ever removes one — a user who names ten thousand sessions over the years
/// would otherwise have every one of them read into memory to answer a question
/// about the last week.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

/// The file session names are recorded in, or `None` when there is no home
/// directory to record them in.
///
/// The same question [`crate::repl::editor::default_history_path`] asks, and
/// answered the same way: a session with nowhere to write its history runs on
/// an in-memory one rather than refusing to start, and a session that cannot
/// record its name says so when it is asked to.
pub fn default_label_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        // A relative `HOME` would put the records beside whatever directory the
        // session happened to be run from.
        return None;
    }
    Some(home.join(".redblue").join(LABEL_FILE))
}

/// Seconds since the Unix epoch, which is what a record's timestamp is.
///
/// Saturation rather than a panic: a system clock before 1970 is a broken clock,
/// and a broken clock is a reason to record the wrong year, not a reason to end
/// the session.
pub fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_secs())
        .unwrap_or(0)
}

/// Whether `name` is a name a record can hold.
///
/// Refused rather than escaped, because the only characters that would need
/// escaping are the two the record format itself is made of: a tab would make
/// one name into two columns and a line break would make it into two sessions.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Error: :session needs a name: a session name cannot be empty.".to_string());
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(format!(
            "Error: a session name is at most {MAX_NAME_BYTES} bytes; that one is {}.",
            name.len()
        ));
    }
    if let Some(found) = name.chars().find(|c| c.is_control()) {
        return Err(format!(
            "Error: a session name cannot contain {:?}, which is not printable text.",
            found
        ));
    }
    Ok(())
}

/// Appends `name` to `path`, stamped with `now`.
///
/// The stamp is passed in rather than read here so that "used this week" is a
/// question about records that a test can write by hand, and not a question
/// about the day the test was run on.
pub fn record(path: &Path, name: &str, now: u64) -> Result<(), String> {
    check_name(name)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Error: :session cannot record a name: {} could not be created: {error}",
                    parent.display()
                )
            })?;
        }
    }
    let written = file_len(path)?;
    if written >= MAX_FILE_BYTES {
        return Err(format!(
            "Error: the session-name file is already {written} bytes, the most it holds; \
             remove it to start a new listing."
        ));
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            format!(
                "Error: :session cannot record a name: {} could not be written: {error}",
                path.display()
            )
        })?;
    writeln!(file, "{now}\t{name}").map_err(|error| {
        format!(
            "Error: :session cannot record a name: {} could not be written: {error}",
            path.display()
        )
    })
}

fn file_len(path: &Path) -> Result<u64, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(format!(
            "Error: :session cannot record a name: {} could not be read: {error}",
            path.display()
        )),
    }
}

/// Every name recorded in `path` in the week ending at `now`, deduplicated and
/// in name order.
///
/// Sorted rather than in the order the records were written, because the order
/// sessions were named in is not a result — it depends on which session was
/// started first, and the listing is asked for across sessions. A name used on
/// Monday and again on Sunday is listed once.
///
/// A line that does not parse is skipped rather than treated as an error: the
/// file is appended to by other sessions, and one that was interrupted
/// mid-write must not make the whole listing unreadable. A file that does not
/// exist is an empty listing, not a failure — that is a session that has never
/// been named.
pub fn used(path: &Path, now: u64) -> Result<Vec<String>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "Error: :sessions cannot read the session names: {} could not be read: {error}",
                path.display()
            ))
        }
    };
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "Error: the session-name file is larger than the {MAX_FILE_BYTES} bytes :sessions \
             reads; remove it to start a new listing."
        ));
    }

    let mut names = BTreeSet::new();
    for line in text.lines() {
        let Some((stamp, name)) = line.split_once('\t') else {
            continue;
        };
        if check_name(name).is_err() {
            continue;
        }
        // A record dated in the future is a record from a clock that disagrees
        // with this one, and it is not evidence of anything having happened yet.
        let Ok(stamp) = stamp.parse::<u64>() else {
            continue;
        };
        if stamp <= now && now - stamp < WEEK_SECONDS {
            names.insert(name.to_string());
        }
    }
    Ok(names.into_iter().collect())
}
