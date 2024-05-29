use std::io::{stdin, stdout, Write};

use anstyle::Style;
use anyhow::Context as _;

use crate::error::CargoResult;
use crate::ops::style::{ERROR, HEADER, NOTE, WARN};

pub fn confirm(prompt: &str) -> bool {
    let mut input = String::new();

    console_println(&format!("{} [y/N] ", prompt), Style::new());

    stdout().flush().unwrap();
    stdin().read_line(&mut input).expect("y/n required");

    input.trim().to_lowercase() == "y"
}

fn console_println(text: &str, style: Style) {
    let _ = writeln!(anstream::stdout(), "{style}{text}{style:#}");
}

/// Print a message with a colored title in the style of Cargo shell messages.
pub fn print(
    status: &str,
    message: impl std::fmt::Display,
    style: Style,
    justified: bool,
) -> CargoResult<()> {
    let mut stderr = anstream::stderr().lock();
    if justified {
        write!(stderr, "{style}{status:>12}{style:#}")?;
    } else {
        write!(stderr, "{style}{}{style:#}:", status)?;
    }

    writeln!(stderr, " {message:#}").with_context(|| "Failed to write message")?;

    Ok(())
}

/// Print a styled action message.
pub fn status(action: &str, message: impl std::fmt::Display) -> CargoResult<()> {
    print(action, message, HEADER, true)
}

/// Print a styled error message.
pub fn error(message: impl std::fmt::Display) -> CargoResult<()> {
    print("error", message, ERROR, false)
}

/// Print a styled warning message.
pub fn warn(message: impl std::fmt::Display) -> CargoResult<()> {
    print("warning", message, WARN, false)
}

/// Print a styled warning message.
pub fn note(message: impl std::fmt::Display) -> CargoResult<()> {
    print("note", message, NOTE, false)
}

pub fn log(level: log::Level, message: impl std::fmt::Display) -> CargoResult<()> {
    match level {
        log::Level::Error => error(message),
        log::Level::Warn => warn(message),
        log::Level::Info => note(message),
        _ => {
            log::log!(level, "{}", message);
            Ok(())
        }
    }
}

/// Print a part of a line with formatting
pub fn write_stderr(fragment: impl std::fmt::Display, style: &Style) -> CargoResult<()> {
    write!(anstream::stderr(), "{style}{fragment}{style:#}")?;
    Ok(())
}
