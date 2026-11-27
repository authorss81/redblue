//! The four surfaces that name the version must name the one the crate is.
//!
//! `rb --version`, `rb version`, the first line of `rb help` and the REPL
//! greeting are written in different places, so a bump can move three of them
//! and leave the fourth behind. Each assertion reads the real binary's output
//! and compares against `env!("CARGO_PKG_VERSION")`, never a written-out
//! number, so the tests move with the crate version.

use std::io::Write;
use std::process::{Command, Stdio};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_rb")
}

fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(binary())
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("REDBLUE_COLOR")
        .output()
        .expect("the rb binary should start")
}

fn stdout_of(args: &[&str]) -> String {
    let output = run(args);
    assert!(
        output.status.success(),
        "`rb {}` should succeed, stderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout should be UTF-8")
}

fn greeting() -> String {
    let mut child = Command::new(binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("NO_COLOR")
        .env_remove("REDBLUE_COLOR")
        .spawn()
        .expect("the rb binary should start");

    child
        .stdin
        .take()
        .expect("stdin should be piped")
        .write_all(b":quit\n")
        .expect("the session should accept its quit command");

    let output = child.wait_with_output().expect("the session should finish");
    assert!(
        output.status.success(),
        "the greeting session should exit 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout should be UTF-8")
}

/// The version token that follows `Redblue v` on a line.
fn version_after(line: &str) -> String {
    let start = line
        .find("Redblue v")
        .map(|i| i + "Redblue v".len())
        .unwrap_or_else(|| panic!("the line names no Redblue version:\n{line:?}"));
    line[start..]
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("the line has no version token:\n{line:?}"))
        .to_string()
}

/// The line that carries the version for one surface.
fn version_line<'a>(text: &'a str, surface: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains("Redblue v"))
        .unwrap_or_else(|| panic!("{surface} names no version:\n{text}"))
}

fn assert_names_crate_version(text: &str, surface: &str) {
    let line = version_line(text, surface);
    let named = version_after(line);
    assert_eq!(
        named,
        crate_version(),
        "{surface} must name the crate version, got {named:?} on:\n{line}"
    );
    assert!(
        !named.is_empty(),
        "{surface} must not carry an empty version"
    );
    assert_eq!(
        line,
        line.trim_end(),
        "{surface} must not carry trailing whitespace:\n{line:?}"
    );
}

#[test]
fn version_flags_name_the_crate_version() {
    for flag in ["--version", "version", "-v"] {
        let output = stdout_of(&[flag]);
        assert_names_crate_version(&output, &format!("rb {flag}"));
    }
}

#[test]
fn help_first_line_names_the_crate_version() {
    let help = stdout_of(&["help"]);
    let first = help.lines().next().expect("help should print a first line");
    assert!(
        first.starts_with(&format!("Redblue v{} - ", crate_version())),
        "the first line of `rb help` must start with the crate version, got:\n{first}"
    );
    assert_names_crate_version(&help, "rb help");
}

/// All four surfaces have to agree with each other and with the crate, so a
/// second spelling cannot appear without failing.
#[test]
fn edge_all_four_version_surfaces_agree() {
    let help = stdout_of(&["help"]);
    let long = stdout_of(&["--version"]);
    let short = stdout_of(&["version"]);
    let session = greeting();

    let surfaces = [
        ("rb help", version_line(&help, "rb help")),
        ("rb --version", version_line(&long, "rb --version")),
        ("rb version", version_line(&short, "rb version")),
        ("repl greeting", version_line(&session, "the repl greeting")),
    ];

    for (surface, line) in surfaces {
        assert_eq!(
            version_after(line),
            crate_version(),
            "{surface} disagrees with the crate version on:\n{line}"
        );
    }
}

/// Every `Redblue v...` occurrence in every surface must be the crate version.
/// This is the binary-level guard that no literal `Redblue v0.1.0` survives.
#[test]
fn edge_no_surface_prints_a_second_version_spelling() {
    let help = stdout_of(&["help"]);
    let long = stdout_of(&["--version"]);
    let short = stdout_of(&["version"]);
    let session = greeting();

    for (surface, text) in [
        ("rb help", help.as_str()),
        ("rb --version", long.as_str()),
        ("rb version", short.as_str()),
        ("repl greeting", session.as_str()),
    ] {
        for line in text.lines().filter(|line| line.contains("Redblue v")) {
            assert_eq!(
                version_after(line),
                crate_version(),
                "{surface} carries a second version spelling on:\n{line}"
            );
        }
    }
}

/// A version command that is given an argument the CLI does not understand is
/// a usage error, not a success that quietly prints a stale banner.
#[test]
fn edge_version_surfaces_reject_extra_arguments() {
    for args in [&["version", "extra"][..], &["help", "extra"][..]] {
        let result = rejection_of(args);
        assert!(
            result.is_err(),
            "`rb {}` should fail, stdout:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&run(args).stdout)
        );
    }
}

/// `Ok(())` only when the command succeeds; `Err` carries the stdout that a
/// successful run would have printed. A version flag with an extra argument
/// must land in the `Err` arm.
fn rejection_of(args: &[&str]) -> Result<(), String> {
    let output = run(args);
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
