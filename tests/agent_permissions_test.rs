//! The opencode configuration the pipeline's agents are resolved out of.
//!
//! `rbops` launches the reviewer and the verifier as `opencode --agent
//! reviewer` and `--agent verifier`. opencode resolves agent definitions from
//! the project config first — `./opencode.json` at the working directory,
//! walked up to the worktree root — and only then from
//! `~/.config/opencode/opencode.json`, with the project deep-merged over the
//! global. Before this file existed the checkout declared no `agent` key
//! anywhere in that path, so neither name resolved and both agents silently
//! ran as the global `default_agent: build`, whose `edit` and `bash` are
//! `allow`. The reviewer is contractually read-only; a permissions gap that
//! makes it writable is invisible to every gate, because nothing the gate runs
//! asks the reviewer to write something.
//!
//! These tests read the shipped file and assert its contents. That is the whole
//! claim they can make: the opencode config schema is documented at
//! <https://opencode.ai/config.json>, and the resolution order above is
//! documented under "Where files live" in the same schema's documentation. A
//! test can prove the file says the right thing; it cannot prove which file
//! opencode opened. What it *can* do is fail the moment a `deny` is dropped,
//! an agent is renamed, or a `default_agent` is quietly reintroduced to
//! repoint the implementer agent.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// The two tools whose permissions this phase exists to pin, per agent.
const TOOLS: [&str; 2] = ["edit", "bash"];

/// The project config filename opencode looks for at the working directory.
const CONFIG_FILE: &str = "opencode.json";

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn config_path() -> PathBuf {
    project_root().join(CONFIG_FILE)
}

/// A path under the build directory that is guaranteed not to exist, so the
/// missing-file case is exercised without touching the filesystem.
fn absent_path() -> PathBuf {
    let path = project_root().join("target/tmp/agent_permissions_absent.json");
    assert!(
        !path.exists(),
        "{} must not exist for this test to mean anything",
        path.display()
    );
    path
}

/// Name a JSON value's type in an error message, so "wrong shape" and "wrong
/// value" do not read the same in a failing gate.
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Assert one agent holds `action` for every tool in [`TOOLS`].
fn check_agent(agents: &Map<String, Value>, name: &str, action: &str) -> Result<(), String> {
    let agent = agents.get(name).ok_or_else(|| {
        format!("`agent.{name}` is not declared: --agent {name} resolves to nothing here")
    })?;
    let permissions = agent.get("permission").ok_or_else(|| {
        format!("`agent.{name}.permission` is not declared: {name} would inherit the global permission block")
    })?;
    let permissions = permissions
        .as_object()
        .ok_or_else(|| format!("`agent.{name}.permission` must be an object keyed by tool name"))?;
    for tool in TOOLS {
        let value = permissions.get(tool).ok_or_else(|| {
            format!("`agent.{name}.permission.{tool}` is not declared: {name} would inherit `permission.{tool}` for {tool}")
        })?;
        let found = value.as_str().ok_or_else(|| {
            format!(
                "`agent.{name}.permission.{tool}` must be a string action, found {}",
                kind(value)
            )
        })?;
        if found != action {
            return Err(format!(
                "`agent.{name}.permission.{tool}` is {found:?}, expected {action:?}"
            ));
        }
    }
    Ok(())
}

/// The whole contract, as one fallible check, so a failure names the exact key
/// that is wrong instead of reporting a pile of assertions.
fn check_document(text: &str) -> Result<(), String> {
    let root: Value = serde_json::from_str(text)
        .map_err(|error| format!("{CONFIG_FILE} is not valid JSON: {error}"))?;
    let root = root
        .as_object()
        .ok_or_else(|| format!("{CONFIG_FILE} must be a JSON object, found {}", kind(&root)))?;
    if let Some(agent) = root.get("default_agent") {
        return Err(format!(
            "{CONFIG_FILE} declares `default_agent`: {}. Every other agent invocation falls back to it, and the implementer agent must stay the one opencode chooses for itself.",
            agent
        ));
    }
    let agents = root
        .get("agent")
        .ok_or_else(|| format!("{CONFIG_FILE} declares no `agent` key: --agent reviewer and --agent verifier both resolve to the global config"))?;
    let agents = agents.as_object().ok_or_else(|| {
        format!(
            "`agent` must be an object keyed by agent name, found {}",
            kind(agents)
        )
    })?;
    check_agent(agents, "reviewer", "deny")?;
    check_agent(agents, "verifier", "allow")?;
    Ok(())
}

/// [`check_document`] on a file, with the path in every message.
fn check_file(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
    check_document(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Read the shipped config, with a message that says what to do about it.
fn shipped_config() -> Value {
    let path = config_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} is missing or unreadable ({error}). opencode resolves a project agent from this file \
             before ~/.config/opencode/opencode.json; without it --agent reviewer and --agent \
             verifier fall through to the global default_agent.",
            path.display()
        )
    });
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", path.display()))
}

/// The file the pipeline actually reads is at the root, where opencode looks
/// first, and not under `.opencode/agent/` — a path this project's gate treats
/// as forbidden.
#[test]
fn project_config_ships_at_the_repository_root() {
    let path = config_path();
    assert!(path.is_file(), "{} must exist as a file", path.display());
    assert_eq!(
        path.parent(),
        Some(project_root().as_path()),
        "{} must sit at the repository root, where opencode looks before the global file",
        path.display()
    );
    assert!(
        !project_root().join(".opencode/agent").join(CONFIG_FILE).exists(),
        "the config must not live under .opencode/agent/, which is a forbidden path for this project"
    );
    assert!(
        !project_root().join(".opencode").join(CONFIG_FILE).exists(),
        "only one project config may be shipped: a second copy under .opencode/ is not consulted"
    );
    check_file(&path)
        .expect("the shipped opencode.json must satisfy the agent permission contract");
}

/// All four permission values, read straight out of the file: `reviewer` denies
/// `edit` and `bash`, `verifier` allows both. Dropping one `deny` fails here.
#[test]
fn project_config_pins_reviewer_read_only_and_verifier_privileged() {
    let agents = shipped_config()
        .get("agent")
        .and_then(Value::as_object)
        .expect("`agent` must be an object keyed by agent name")
        .clone();
    for (name, action) in [("reviewer", "deny"), ("verifier", "allow")] {
        let permissions = agents
            .get(name)
            .and_then(|agent| agent.get("permission"))
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("`agent.{name}.permission` must be an object"));
        for tool in TOOLS {
            assert_eq!(
                permissions.get(tool).and_then(Value::as_str),
                Some(action),
                "`agent.{name}.permission.{tool}` must be {action:?}"
            );
        }
    }
}

/// No `default_agent`: the implementer agent is opencode's business, and a
/// key here could repoint it without any gate noticing.
#[test]
fn project_config_declares_no_default_agent() {
    let config = shipped_config();
    assert!(
        !config
            .as_object()
            .expect("the config must be an object")
            .contains_key("default_agent"),
        "opencode.json must not declare `default_agent`: it would repoint the agent every phase runs as"
    );
}

/// The absence of the file is the failure this phase fixes.
#[test]
fn edge_missing_config_file_is_rejected() {
    let path = absent_path();
    let error =
        check_file(&path).expect_err("a config that is not there cannot pin any permission");
    assert!(
        error.contains("cannot be read"),
        "the error must say the file could not be read, got: {error}"
    );
}

/// An empty document, and a document whose `agent` block is gone: both leave
/// `--agent reviewer` resolving to the global config.
#[test]
fn edge_missing_agent_key_is_rejected() {
    for (label, text) in [
        ("empty document", "{}"),
        ("agent key absent", r#"{"$schema":"x"}"#),
    ] {
        let error = check_document(text).expect_err(&format!("{label} declares no agents"));
        assert!(
            error.contains("`agent` key"),
            "{label}: the error must name the missing `agent` key, got: {error}"
        );
    }
}

/// Only the verifier declared is still a gap: the reviewer falls back.
#[test]
fn edge_missing_reviewer_is_rejected() {
    let text = r#"{"agent":{"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#;
    let error =
        check_document(text).expect_err("a config with no reviewer cannot hold it read-only");
    assert!(
        error.contains("`agent.reviewer` is not declared"),
        "the error must name the missing reviewer, got: {error}"
    );
}

/// A `default_agent` in the project config is the one key that can repoint the
/// agent every phase runs as, so it is rejected whatever it names.
#[test]
fn edge_default_agent_key_is_rejected() {
    for name in ["build", "reviewer"] {
        let mut config = shipped_config();
        config["default_agent"] = Value::String(name.to_string());
        let error = check_document(&config.to_string())
            .expect_err("a `default_agent` must not be accepted, whatever it names");
        assert!(
            error.contains("`default_agent`"),
            "the error must name the offending key, got: {error}"
        );
    }
}

/// The `agent` key present but unusable is the same failure as its absence: no
/// name resolves.
#[test]
fn edge_agent_block_of_wrong_type_is_rejected() {
    for (label, text) in [
        ("an array of names", r#"{"agent":["reviewer","verifier"]}"#),
        ("a bare string", r#"{"agent":"reviewer"}"#),
    ] {
        let error =
            check_document(text).expect_err(&format!("`agent` holding {label} declares no agents"));
        assert!(
            error.contains("`agent` must be an object"),
            "{label}: the error must explain the expected shape, got: {error}"
        );
    }
}

/// Only the reviewer declared is the same gap for the verifier, which has to
/// run the gate.
#[test]
fn edge_missing_verifier_is_rejected() {
    let text = r#"{"agent":{"reviewer":{"permission":{"edit":"deny","bash":"deny"}}}}"#;
    let error = check_document(text).expect_err("a config with no verifier cannot run the gate");
    assert!(
        error.contains("`agent.verifier` is not declared"),
        "the error must name the missing verifier, got: {error}"
    );
}

/// The single most damaging edit: one `deny` becomes `allow`. The shipped file
/// is mutated through a real JSON value, not retyped, so the test cannot drift
/// from the document it is a copy of.
#[test]
fn edge_permission_flipped_to_allow_is_rejected() {
    let config = shipped_config();
    for (name, tool, flipped) in [
        ("reviewer", "edit", "allow"),
        ("reviewer", "bash", "allow"),
        ("verifier", "edit", "deny"),
        ("verifier", "bash", "deny"),
    ] {
        let mut config = config.clone();
        config["agent"][name]["permission"][tool] = Value::String(flipped.to_string());
        let error = check_document(&config.to_string())
            .expect_err("a flipped permission must be caught, whatever it was flipped to");
        assert!(
            error.contains(&format!("`agent.{name}.permission.{tool}`")),
            "the error must name the flipped key, got: {error}"
        );
    }
}

/// An agent named but granted nothing.
#[test]
fn edge_reviewer_without_permission_block_is_rejected() {
    let text = r#"{"agent":{"reviewer":{"description":"read only"},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#;
    let error = check_document(text)
        .expect_err("a permission-less reviewer inherits whatever the global config grants");
    assert!(
        error.contains("`agent.reviewer.permission` is not declared"),
        "the error must name the missing permission block, got: {error}"
    );
}

/// A permission whose value is the wrong type is not a permission opencode can
/// apply, and reading it as one would silently widen the reviewer.
#[test]
fn edge_permission_value_of_wrong_type_is_rejected() {
    for (label, text) in [
        (
            "boolean",
            r#"{"agent":{"reviewer":{"permission":{"edit":true,"bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
        ),
        (
            "number",
            r#"{"agent":{"reviewer":{"permission":{"edit":"deny","bash":0}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
        ),
        (
            "nested object",
            r#"{"agent":{"reviewer":{"permission":{"edit":{"*":"allow"},"bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
        ),
        (
            "null permission block",
            r#"{"agent":{"reviewer":{"permission":null},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
        ),
    ] {
        let error = check_document(text).expect_err(&format!(
            "a {label} permission must not be read as an action"
        ));
        assert!(
            error.contains("must be"),
            "{label}: the error must explain the expected shape, got: {error}"
        );
    }
}

/// A repeated key resolves last-wins, which for `edit` on the reviewer means a
/// trailing `"allow"` is the one that counts. Pinned so a hand-merged file
/// cannot hide behind the first occurrence.
#[test]
fn edge_duplicate_permission_key_resolves_to_the_last_value() {
    let text = r#"{"agent":{"reviewer":{"permission":{"edit":"allow","edit":"allow","bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#;
    let error =
        check_document(text).expect_err("the surviving value is allow, whatever came before it");
    assert!(
        error.contains("`agent.reviewer.permission.edit` is \"allow\""),
        "the error must report the value that survives, got: {error}"
    );
}

/// Malformed files, including the ones a half-written checkout produces.
#[test]
fn edge_malformed_config_is_rejected() {
    for (label, text) in [
        ("empty file", ""),
        ("truncated", r#"{"agent":{"reviewer":"#),
        ("trailing comma", r#"{"agent":{},}"#),
        ("byte order mark", "\u{feff}{\"agent\":{}}"),
        ("not an object", r#"["agent"]"#),
        ("a bare string", r#""reviewer""#),
    ] {
        let error =
            check_document(text).expect_err(&format!("a {label} config must not be accepted"));
        assert!(
            error.contains(CONFIG_FILE) || error.contains("must be an object"),
            "{label}: the error must say what is wrong with the file, got: {error}"
        );
    }
}
