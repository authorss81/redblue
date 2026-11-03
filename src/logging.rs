use std::cell::RefCell;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
    Off,
}

impl Level {
    pub fn parse(name: &str) -> Option<Level> {
        match name {
            "debug" => Some(Level::Debug),
            "info" => Some(Level::Info),
            "warn" => Some(Level::Warn),
            "error" => Some(Level::Error),
            "off" => Some(Level::Off),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
            Level::Off => "off",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Off => "OFF",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Destination {
    Stdout,
    Stderr,
    File(String),
}

#[derive(Clone, Debug)]
struct Config {
    threshold: Level,
    destination: Destination,
    timestamps: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            threshold: Level::Off,
            destination: Destination::Stderr,
            timestamps: true,
        }
    }
}

thread_local! {
    static CONFIG: RefCell<Config> = RefCell::new(Config::default());
}

pub fn reset() {
    CONFIG.with(|config| *config.borrow_mut() = Config::default());
}

pub fn set_level(name: &str) -> Result<(), String> {
    let level = Level::parse(name).ok_or_else(|| {
        format!(
            "logging.set_level requires one of debug, info, warn, error or off, \
             but was given '{name}'"
        )
    })?;
    CONFIG.with(|config| config.borrow_mut().threshold = level);
    Ok(())
}

pub fn level() -> String {
    CONFIG.with(|config| config.borrow().threshold.name().to_string())
}

pub fn set_timestamps(on: bool) {
    CONFIG.with(|config| config.borrow_mut().timestamps = on);
}

pub fn set_destination(destination: &str) -> Result<(), String> {
    if destination.is_empty() {
        return Err("logging.set_destination requires a non-empty destination".to_string());
    }
    let destination = match destination {
        "stdout" => Destination::Stdout,
        "stderr" => Destination::Stderr,
        path => Destination::File(path.to_string()),
    };
    CONFIG.with(|config| config.borrow_mut().destination = destination);
    Ok(())
}

pub fn enabled(level: Level) -> bool {
    CONFIG.with(|config| {
        let config = config.borrow();
        level != Level::Off && config.threshold != Level::Off && level >= config.threshold
    })
}

/// A timestamp for the current record, from the same clock `time.now()` reads.
///
/// A host that has no clock — the playground before `rb_set_clock` — gets a
/// record without a timestamp rather than an abort. That is the one place the
/// two differ: `time.now()` answers an error when there is no clock, while a
/// logging call still has something to say and must not take the run down.
fn format_timestamp() -> Option<String> {
    format_timestamp_at(crate::runtime::current_instant().ok())
}

/// Formats `instant`, or answers `None` when there is no clock or it names a
/// moment `chrono` cannot represent.
///
/// Kept separate so the no-clock and out-of-range paths are testable on the
/// native target, where the runtime always has a clock.
fn format_timestamp_at(instant: Option<SystemTime>) -> Option<String> {
    let since_epoch = instant?.duration_since(UNIX_EPOCH).ok()?;
    let secs = i64::try_from(since_epoch.as_secs()).ok()?;
    Some(
        chrono::DateTime::<chrono::Utc>::from_timestamp(secs, since_epoch.subsec_nanos())?
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
    )
}

pub fn write(level: Level, message: &str) -> io::Result<()> {
    let (destination, timestamps) = CONFIG.with(|config| {
        let config = config.borrow();
        (config.destination.clone(), config.timestamps)
    });
    if !enabled(level) {
        return Ok(());
    }

    let timestamp = if timestamps { format_timestamp() } else { None };
    let record = format_record(level, message, timestamp.as_deref());

    match destination {
        Destination::Stdout => write_stdout(&record),
        Destination::Stderr => write_stderr(&record),
        Destination::File(path) => crate::vfs::append(&path, &record),
    }
}

/// Writes a record to the real descriptor and reports a failed write.
///
/// `std::io::stderr().write_all` treats `EBADF` as success, so a descriptor
/// that is genuinely closed would still lose the record silently. Writing to
/// the descriptor directly keeps a closed stream a failure the program can
/// catch.
#[cfg(unix)]
fn write_descriptor(fd: libc::c_int, record: &str) -> io::Result<()> {
    let bytes = record.as_bytes();
    let mut written = 0;
    while written < bytes.len() {
        let chunk = &bytes[written..];
        // SAFETY: `chunk` is a valid slice for `chunk.len()` bytes and `fd` is
        // one of the process's standard stream descriptors.
        let count = unsafe { libc::write(fd, chunk.as_ptr().cast(), chunk.len()) };
        if count < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "the log destination accepted no bytes",
            ));
        }
        written += count as usize;
    }
    Ok(())
}

#[cfg(unix)]
fn write_stdout(record: &str) -> io::Result<()> {
    if crate::wasm::is_capturing() {
        return crate::wasm::try_emit(record);
    }
    let _ = io::Write::flush(&mut io::stdout());
    write_descriptor(libc::STDOUT_FILENO, record)
}

#[cfg(unix)]
fn write_stderr(record: &str) -> io::Result<()> {
    write_descriptor(libc::STDERR_FILENO, record)
}

#[cfg(not(unix))]
fn write_stdout(record: &str) -> io::Result<()> {
    crate::wasm::try_emit(record)
}

#[cfg(not(unix))]
fn write_stderr(record: &str) -> io::Result<()> {
    io::Write::write_all(&mut io::stderr(), record.as_bytes())
}

pub fn format_record(level: Level, message: &str, timestamp: Option<&str>) -> String {
    match timestamp {
        Some(timestamp) => format!("[{timestamp}] [{}] {message}\n", level.label()),
        None => format!("[{}] {message}\n", level.label()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn edge_no_clock_means_a_record_without_a_timestamp() {
        assert_eq!(format_timestamp_at(None), None);
    }

    #[test]
    fn a_known_instant_is_formatted_in_utc_to_the_millisecond() {
        assert_eq!(
            format_timestamp_at(Some(UNIX_EPOCH + Duration::from_millis(1_234))).as_deref(),
            Some("1970-01-01T00:00:01.234Z")
        );
    }

    #[test]
    fn edge_an_instant_past_chronos_range_means_no_timestamp() {
        let instant = UNIX_EPOCH
            .checked_add(Duration::from_secs(10_000_000_000_000))
            .expect("the native clock represents this instant");
        assert_eq!(format_timestamp_at(Some(instant)), None);
    }
}
