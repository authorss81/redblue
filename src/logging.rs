use std::cell::RefCell;
use std::io;

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

pub fn write(level: Level, message: &str) -> io::Result<()> {
    let (destination, timestamps) = CONFIG.with(|config| {
        let config = config.borrow();
        (config.destination.clone(), config.timestamps)
    });
    if !enabled(level) {
        return Ok(());
    }

    let timestamp = timestamps.then(|| {
        chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string()
    });
    let record = format_record(level, message, timestamp.as_deref());

    match destination {
        Destination::Stdout => {
            crate::wasm::emit(&record);
            Ok(())
        }
        Destination::Stderr => {
            eprint!("{record}");
            Ok(())
        }
        Destination::File(path) => crate::vfs::append(&path, &record),
    }
}

pub fn format_record(level: Level, message: &str, timestamp: Option<&str>) -> String {
    match timestamp {
        Some(timestamp) => format!("[{timestamp}] [{}] {message}\n", level.label()),
        None => format!("[{}] {message}\n", level.label()),
    }
}
