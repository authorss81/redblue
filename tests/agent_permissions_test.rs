//! The opencode configuration the pipeline's agents are resolved out of.
//!
//! `rbops` launches the reviewer as `opencode --agent reviewer`, and the
//! implementer as `--agent build`. opencode resolves agent definitions from the
//! project config first — `./opencode.json` at the working directory, walked up
//! to the worktree root — and only then from `~/.config/opencode/opencode.json`,
//! with the project deep-merged over the global. Before this file existed the
//! checkout declared no `agent` key anywhere in that path, so neither name
//! resolved and both agents silently ran as the global `default_agent: build`,
//! whose `edit` and `bash` are `allow`. The reviewer is contractually
//! read-only; a permissions gap that makes it writable is invisible to every
//! gate, because nothing the gate runs asks the reviewer to write something.
//!
//! Two things bind `--agent reviewer` to this file's agent, and both are
//! asserted here:
//!
//! - the **permission block**, which decides what the reviewer may do; and
//! - the **mode**, which decides whether the name resolves to this agent at
//!   all. opencode binds `--agent <name>` to a *primary* agent: a name that
//!   resolves to a subagent makes opencode print `agent "reviewer" is a
//!   subagent, not a primary agent. Falling back to default agent` and run the
//!   default agent instead — here `build`, whose `edit` and `bash` are
//!   `allow`. A permission block that is never bound is not a permission
//!   block.
//!
//! `agent.verifier` is declared here, but no `--agent verifier` exists in
//! `rbops` today — the pipeline runs its gates as a script, not as an agent. It
//! is declared because a dispatcher that runs a phase's gates as an agent must
//! resolve to *these* permissions rather than to the global build agent, and
//! because the declaration is what makes that wiring a one-word change instead
//! of a second permissions hole. The tests below pin the declaration; they do
//! not claim the pipeline launches it.
//!
//! These tests read the shipped file and assert its contents. That is the whole
//! claim they can make: the opencode config schema is documented at
//! <https://opencode.ai/config.json>, and the resolution order above is
//! documented in opencode's configuration reference
//! (<https://opencode.ai/docs/config>, section "Locations") and in the
//! `customize-opencode` skill opencode ships (section "Where files live").
//! A test can prove the file says the right thing; it cannot prove which file
//! opencode opened. What it *can* do is fail the moment a `deny` is dropped,
//! a `deny` is narrowed to a pattern that leaves the rest of the paths to the
//! rules above it, an agent is renamed, a mode is changed, or a
//! `default_agent` is quietly reintroduced to repoint the implementer agent.
//!
//! Two further rules keep the file from reaching past the agent it is about.
//! opencode deep-merges the project config over the global one, per key, so
//! both of these would move a permission for an agent the file does not
//! declare: a **top-level `permission` block**, which applies to every agent
//! that declares no block of its own, and an **agent this file does not own**
//! — `build` above all, since the pipeline runs every phase's implementer as
//! `--agent build` and a block for that name here would replace the global
//! one for it.
//!
//! The same reach is closed by two ALLOW-LISTS, one per level of the
//! document: the top-level keys the file may declare (`$schema` and
//! `agent`), and the keys an agent block may declare (`description`, `mode`
//! and `permission`). `instructions` is the sharp one at the top level — it
//! is prepended to the system prompt of every agent in the project — and
//! `prompt` is the same reach one level down: it is the agent's own prompt,
//! so a line there is a line the reviewer reads as an instruction from the
//! pipeline rather than from its own contract.
//!
//! The reviewer's permission block is closed as well, in both directions: it
//! declares **exactly** the eleven keys it must deny. `read` is not one of
//! them, and declaring it would be a widening rather than a statement of
//! intent — opencode's default `read` rules ask about `.env` files, and an
//! agent-level `read: "allow"` is appended after them and wins the
//! last-match.
//!
//! The verifier's block is closed the same way, in the other direction: it
//! declares **exactly** the two tools it needs — `edit` and `bash`, both
//! allowed — and nothing else. A third key there is a grant no assertion in
//! this suite reads, and a denial there would take the toolchain away: the
//! verifier runs the gates, and cargo reads `~/.cargo` while the toolchain
//! lives in `~/.rustup`, outside the checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value};

/// The two tools whose permissions this phase exists to pin, per agent.
const TOOLS: [&str; 2] = ["edit", "bash"];

/// Every permission key the reviewer must deny, and why each one is named.
///
/// The list is opencode's own key set. The schema types `AgentConfig.permission`
/// with `read`, `edit`, `glob`, `grep`, `list`, `bash`, `task`,
/// `external_directory`, `todowrite`, `question`, `webfetch`, `websearch`,
/// `lsp`, `doom_loop` and `skill`; every one of them that is not a read-only
/// tool is here.
///
/// - `edit` covers `write`, `edit` and `apply_patch` — one key, not three.
/// - `bash` is the shell the reviewer has no business running.
/// - `task` is what stops the reviewer from delegating to a subagent, and the
///   built-in `general` subagent has full tool access.
/// - `webfetch` and `websearch` are the two ways out of the checkout: one is
///   granted by the global config, the other by opencode's permissive default.
/// - `skill` loads a skill file into the reviewer's context. A skill is
///   instructions the reviewer then follows, and it can be fetched from disk
///   or from a URL — a grant, not a read.
/// - `lsp` runs language-server queries. The reviewer has no use for it and no
///   reason to hold it.
/// - `question` asks a human. A pipeline review has no human to ask, and the
///   answer would be the only way to widen the reviewer.
/// - `todowrite` writes to the session, not to the tree — but it is still a
///   grant, and the reviewer is granted nothing it does not need.
/// - `doom_loop` is opencode's repeated-call guard. It defaults to `ask`, and
///   the pipeline runs `opencode run` with nobody to answer a prompt, so it is
///   denied outright: the guard blocks the repeated call instead of waiting
///   for an answer that will never come.
/// - `external_directory` is the one the global config grants as
///   `{"**": "allow"}`.
///
/// opencode's defaults are permissive ("Most permissions default to `allow`"),
/// so a key that is not declared is a key that is allowed.
const REVIEWER_DENIED: [&str; 11] = [
    "edit",
    "bash",
    "task",
    "todowrite",
    "skill",
    "lsp",
    "question",
    "webfetch",
    "websearch",
    "doom_loop",
    "external_directory",
];

/// The permission keys the reviewer must not declare AT ALL.
///
/// These are the read-only tools its job needs, and opencode already allows
/// them by default. The default is the safer of the two answers available
/// here, so the shipped file declares none of them — and the checker refuses
/// any of them, whatever value they carry.
///
/// Declaring `read: "allow"` would be an *widening* rather than a statement
/// of intent: opencode's default `read` rules ASK about `.env` files
/// (`read *.env ask`), and an agent-level `read: "allow"` is appended after
/// them, so it would win the last-match and re-allow them. Declaring
/// `read: "deny"` would take the reviewer's own job away — it is there to
/// read the diff. The default is the safer of the two here: the pipeline
/// runs `opencode run` with nobody to answer a prompt, so an `ask` is a
/// request that cannot be granted — the same reason `doom_loop` is denied
/// outright rather than left to ask.
///
/// So the reviewer's permission block is exactly the eleven denied keys, and
/// a `read`/`glob`/`grep`/`list` key in it is refused in either direction.
const REVIEWER_UNDECLARED: [&str; 4] = ["read", "glob", "grep", "list"];

/// Every permission key the published schema types a permission block with.
///
/// `AgentConfig.permission` is a `PermissionConfig`, which is either a bare
/// action or an object with exactly these fifteen properties — re-measured
/// against the schema at <https://opencode.ai/config.json>, whose
/// `PermissionConfig` is an `anyOf` of a bare action and an object with these
/// keys: `bash`, `doom_loop`, `edit`, `external_directory`, `glob`, `grep`,
/// `list`, `lsp`, `question`, `read`, `skill`, `task`, `todowrite`,
/// `webfetch`, `websearch`. The reviewer's block accounts for all of them:
/// eleven are denied by name ([`REVIEWER_DENIED`]) and four are deliberately
/// left undeclared ([`REVIEWER_UNDECLARED`]), because opencode already allows
/// the read-only ones by default and declaring them would be a widening rather
/// than a statement of intent.
///
/// **What this list is not.** It is not every permission opencode *resolves*.
/// The resolved ruleset carries two entries that this list does not name —
/// `plan_enter` and `plan_exit` — and the schema does not type them as
/// permission keys at all (`grep -c plan_enter` against the published schema
/// is `0`), so an agent's permission block cannot configure them. opencode
/// denies both in its own defaults, for the reviewer and the verifier alike;
/// this file's block is not what denies them, and nothing here claims it is.
///
/// The list is here so that "the reviewer is read-only" can be checked as a
/// COMPLETE claim rather than a partial one — a complete claim **over the
/// keys the schema types**, which is the set this file can act on at all.
/// The two lists are not two independent sets: they are a partition of this
/// constant.
///
/// **It cannot detect a key opencode publishes tomorrow.** The key set is
/// written out here, and the schema is not read at test time, so a key
/// published tomorrow lands in neither list and nothing in this suite says
/// so. What the test below asserts is the partition of THIS constant, and
/// the claim it supports is the one this file can support: every key this
/// suite knows about is either denied by name or deliberately left
/// undeclared. The live measurement (`opencode debug agent reviewer`) is what
/// shows a key opencode has added.
const PERMISSION_KEYS: [&str; 15] = [
    "bash",
    "doom_loop",
    "edit",
    "external_directory",
    "glob",
    "grep",
    "list",
    "lsp",
    "question",
    "read",
    "skill",
    "task",
    "todowrite",
    "webfetch",
    "websearch",
];

/// The permission keys whose schema type is the bare action only.
///
/// opencode types the rest of the keys as `PermissionRuleConfig`, which is
/// either a bare action or an object of pattern → action. These five are typed
/// as the action alone, so an object of patterns is not a value opencode can
/// apply to them.
const STRING_ONLY_KEYS: [&str; 5] = [
    "todowrite",
    "question",
    "webfetch",
    "websearch",
    "doom_loop",
];

/// The mode an agent must declare to be selectable as `--agent <name>`.
const PRIMARY_MODE: &str = "primary";

/// The patterns that cover everything a permission key governs.
///
/// A bare action is opencode's own shorthand for the catch-all rule
/// (`"deny"` is `{"*": "deny"}`), and the global config writes the same
/// catch-all as `{"**": "allow"}`. A pattern that is neither of those covers
/// only what it matches.
const CATCH_ALL_PATTERNS: [&str; 2] = ["*", "**"];

/// The agents this file is allowed to declare.
///
/// The project config is the pipeline's agent file: it declares the reviewer
/// and the verifier, and nothing else. A third name is the hazard — `build`
/// above all, because `rbops` runs every phase's implementer as
/// `--agent build`. opencode resolves a name in the project config first and
/// only then in the global one, so a `build` block here would OVERRIDE the
/// global config for that name: the implementer's permissions would move
/// without the global file changing, and without any other gate noticing.
const PIPELINE_AGENTS: [&str; 2] = ["reviewer", "verifier"];

/// The top-level keys this file is allowed to declare.
///
/// The file has exactly one job: to give the pipeline's two agents their
/// permissions. opencode deep-merges the project config over the global one
/// **per key**, so every top-level key here reaches EVERY agent in the
/// project — the implementer's included — and not only the two this file
/// names. `permission` and `default_agent` are refused by name for exactly
/// that reason, and the rest of opencode's top-level keys reach just as far.
///
/// `instructions` is the sharp one. It is prepended to the system prompt of
/// every agent in the project, so a line there is a line the reviewer reads as
/// an instruction from the pipeline rather than from its own contract — and
/// the reviewer's contract is the one thing this file exists to protect.
///
/// `$schema` names the schema the file is written against, and `agent` holds
/// the agents themselves. Nothing else belongs here.
const TOP_LEVEL_KEYS: [&str; 2] = ["$schema", "agent"];

/// The keys an agent block in this file is allowed to declare.
///
/// The top-level allow-list, one level down. The published schema types
/// `agent.<name>` as `AgentConfig`, whose properties are `color`,
/// `description`, `disable`, `hidden`, `maxSteps`, `mode`, `model`,
/// `options`, `permission`, `prompt`, `steps`, `temperature`, `tools`,
/// `top_p` and `variant` — and opencode deep-merges the project config over
/// the global one for THAT agent, so every one of them is part of the
/// agent's contract with the pipeline.
///
/// Three of them are this file's to set:
///
/// - `permission` is the contract itself;
/// - `mode` is what binds `--agent <name>` to the agent at all; and
/// - `description` says when to use the agent, and grants nothing.
///
/// `prompt` is the sharp one. It is the agent's own prompt, so a line there
/// is a line the reviewer reads as an instruction from the pipeline rather
/// than from its own contract — the same reach the top-level `instructions`
/// key has, one level down. `tools` is the pre-`permission` way of switching
/// a tool on for one agent; it is deprecated, but it is still read, and it
/// grants. The rest — `model`, `options`, `temperature`, `top_p`, `maxSteps`,
/// `steps`, `variant`, `color`, `disable`, `hidden` — are not this file's
/// business, and a key that is not on this list is a key that is not
/// checked.
const AGENT_KEYS: [&str; 3] = ["description", "mode", "permission"];

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

/// Run `git` in this checkout and return its exit status, its stdout and its
/// stderr.
///
/// The status is returned rather than raised, because the check that uses this
/// is ABOUT a non-zero status: `git check-ignore` answers "not ignored" with
/// status 1, and that answer is the one it needs.
///
/// stderr comes back as well as stdout because that is where git writes the
/// REASON: `git ls-files --error-unmatch` exits 1 with the reason on stderr
/// and nothing at all on stdout, so an error message built from stdout alone
/// says "failed with status 1: " and names no reason whatsoever.
fn git_run(args: &[&str]) -> Result<(i32, String, String), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(project_root())
        .args(args)
        .output()
        .map_err(|e| format!("could not run `git {}`: {e}", args.join(" ")))?;
    let status = out.status.code().unwrap_or(-1);
    Ok((
        status,
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// Run `git` in this checkout and return its stdout, or a readable reason.
///
/// A failed invocation is never turned into "nothing found": that is the false
/// pass this whole file is built to avoid. The reason git gave is in the
/// message — stderr first, because that is where git writes it, and stdout
/// only when git wrote nothing there.
fn git(args: &[&str]) -> Result<String, String> {
    let (status, stdout, stderr) = git_run(args)?;
    if status != 0 {
        let reason = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(format!(
            "`git {}` failed with status {status}: {reason}",
            args.join(" ")
        ));
    }
    Ok(stdout)
}

/// The action a permission value applies, or the reason it does not resolve.
///
/// opencode accepts two shapes for a permission value: a bare action
/// (`"deny"`), and an object of pattern → action (`{"**": "deny"}`). The bare
/// action is opencode's shorthand for the catch-all rule `{"*": action}`, and
/// an object is the same rule as the bare one only when every pattern in it
/// carries the same action AND one of those patterns is a catch-all (`*` or
/// `**`).
///
/// A narrow pattern is NOT the same rule, and the difference is the whole
/// point. opencode evaluates the LAST matching rule, so `{"*.md": "deny"}`
/// denies the markdown files and leaves every other path to the rules above
/// it — and in this checkout the global config's `external_directory
/// {"**": "allow"}` sits above the agent's own rules. Reading that object as
/// the word "deny" is how a reviewer that may read outside the checkout
/// passes a suite that says it may not, so a narrow object is refused here
/// rather than read as the action it spells.
///
/// A mixed object is not one action either — it denies some patterns and
/// allows others — so it is rejected rather than read as whichever action
/// happened to come first.
fn permission_action<'a>(key: &str, value: &'a Value) -> Result<&'a str, String> {
    match value {
        Value::String(action) => Ok(action.as_str()),
        Value::Object(patterns) => {
            if STRING_ONLY_KEYS.contains(&key) {
                return Err(format!(
                    "takes a bare action: opencode's schema types `{key}` as one, and an object \
                     of patterns is not a value it can apply"
                ));
            }
            let mut action: Option<&str> = None;
            for (pattern, value) in patterns {
                let value = value.as_str().ok_or_else(|| {
                    format!(
                        "the pattern {pattern:?} must be a string action, found {}",
                        kind(value)
                    )
                })?;
                match action {
                    None => action = Some(value),
                    Some(previous) if previous == value => {}
                    Some(previous) => {
                        return Err(format!(
                            "the patterns disagree: {previous:?} and {value:?} — a mixed object is \
                             not one action"
                        ))
                    }
                }
            }
            let action = action
                .ok_or_else(|| "an empty object grants nothing and is not an action".to_string())?;
            if !patterns
                .keys()
                .any(|pattern| CATCH_ALL_PATTERNS.contains(&pattern.as_str()))
            {
                let names: Vec<&str> = patterns.keys().map(String::as_str).collect();
                return Err(format!(
                    "the patterns {names:?} do not cover every path: an object of patterns is the \
                     same rule as a bare action only when one of them is a catch-all (`*` or \
                     `**`) — a narrow pattern decides only the paths it matches, and every other \
                     path falls through to the rules above it"
                ));
            }
            Ok(action)
        }
        other => Err(format!("must be a string action, found {}", kind(other))),
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
        let found = permission_action(tool, value)
            .map_err(|error| format!("`agent.{name}.permission.{tool}` {error}"))?;
        if found != action {
            return Err(format!(
                "`agent.{name}.permission.{tool}` is {found:?}, expected {action:?}"
            ));
        }
    }
    Ok(())
}

/// Require an agent to declare `mode: "primary"`.
///
/// `--agent <name>` names a TOP-LEVEL agent. opencode resolves the name and,
/// when the agent it finds is a subagent, prints `agent "reviewer" is a
/// subagent, not a primary agent. Falling back to default agent` and runs the
/// default agent instead — which here is `build`, whose `edit` and `bash` are
/// `allow`. The permission block would then be read and never applied, so the
/// mode is as load-bearing as the permissions it guards.
fn check_mode(agents: &Map<String, Value>, name: &str) -> Result<(), String> {
    let agent = agents
        .get(name)
        .ok_or_else(|| format!("`agent.{name}` is not declared"))?;
    let mode = agent.get("mode").ok_or_else(|| {
        format!("`agent.{name}.mode` is not declared: --agent {name} binds nothing without it")
    })?;
    let found = mode.as_str().ok_or_else(|| {
        format!(
            "`agent.{name}.mode` must be a string mode, found {}",
            kind(mode)
        )
    })?;
    if found != PRIMARY_MODE {
        return Err(format!(
            "`agent.{name}.mode` is {found:?}, expected {PRIMARY_MODE:?}: --agent {name} names a \
             top-level agent, and opencode binds the name to a primary one"
        ));
    }
    Ok(())
}

/// Require the reviewer's permission block to declare exactly the denied keys.
///
/// The reviewer's permission block is an ALLOW-LIST of keys, and this is the
/// check that closes it in BOTH directions:
///
/// - Every key the block declares must be one of [`REVIEWER_DENIED`], and it
///   must deny it. A key outside that set is refused whether it grants or
///   denies: a grant is a reviewer that can act, and a key this file does
///   not own is a key no assertion in this suite reads.
///   [`check_agent`] looks up exactly [`TOOLS`], so it would never visit
///   such a key on its own.
/// - The four keys in [`REVIEWER_UNDECLARED`] are refused outright, whatever
///   their value. `read: "allow"` is the widening this phase's own
///   documentation names: opencode's default `read` rules ask about `.env`
///   files (`read *.env ask`), and an agent-level `read: "allow"` is
///   appended after them, so it wins the last-match and re-allows them.
///   `read: "deny"` would take the reviewer's own job away. The default is
///   the safer of the two, and the default is what the file leaves alone.
///
/// Together with [`check_reviewer_denies`] — which requires every one of the
/// eleven to be PRESENT — the two make the reviewer's block exactly the
/// eleven keys it must deny, and nothing else.
fn check_no_grants_beyond(agents: &Map<String, Value>, name: &str) -> Result<(), String> {
    let permissions = agents
        .get(name)
        .and_then(|agent| agent.get("permission"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("`agent.{name}.permission` must be an object"))?;
    for (tool, value) in permissions {
        // The value's shape is read first: a value opencode cannot apply is
        // malformed whatever key it sits under, and saying so names the more
        // specific fault.
        let found = permission_action(tool, value)
            .map_err(|error| format!("`agent.{name}.permission.{tool}` {error}"))?;
        if REVIEWER_UNDECLARED.contains(&tool.as_str()) {
            return Err(format!(
                "`agent.{name}.permission.{tool}` is declared: {name} must not declare \
                 `{tool}` at all. opencode already allows it by default, and declaring it is \
                 a WIDENING, not a statement of intent - opencode's default `read` rules ask \
                 about `.env` files (`read *.env ask`), and an agent-level `read: \"allow\"` is \
                 appended after them, so it wins the last-match and re-allows them. The default \
                 is the safer of the two: the pipeline runs `opencode run` with nobody to answer \
                 a prompt, so an `ask` is a request that cannot be granted."
            ));
        }
        if !REVIEWER_DENIED.contains(&tool.as_str()) {
            return Err(format!(
                "`agent.{name}.permission.{tool}` is declared: {name} is read-only, so its \
                 permission block declares exactly the {} keys it must deny - {} - and nothing \
                 else",
                REVIEWER_DENIED.len(),
                REVIEWER_DENIED.join(", ")
            ));
        }
        if found != "deny" {
            return Err(format!(
                "`agent.{name}.permission.{tool}` is {found:?}: {name} is read-only, so every \
                 key it declares must deny"
            ));
        }
    }
    Ok(())
}

/// Require every key in [`REVIEWER_DENIED`] to be present AND to deny.
///
/// This is the check the other two cannot make. [`check_agent`] looks up
/// exactly [`TOOLS`], and [`check_no_grants_beyond`] visits only the keys that
/// ARE present — so a key that was never declared passes both. An undeclared
/// key is not denied by the file: opencode's defaults are permissive and the
/// global config grants `webfetch` and `external_directory`, so the key comes
/// back as allowed.
///
/// "Deny" here means the bare action, or the object form of it that covers
/// everything — [`permission_action`] refuses a narrow object, so
/// `{"*.md": "deny"}` cannot pass as a deny here. A deny that covers only
/// the paths it matches leaves the rest of them to the rules above it.
///
/// This and [`check_no_grants_beyond`] are the two halves of one rule: this
/// one requires each of the eleven to be PRESENT, that one requires every key
/// that IS present to be one of the eleven. Together they make the
/// reviewer's permission block exactly the eleven keys.
fn check_reviewer_denies(agents: &Map<String, Value>, name: &str) -> Result<(), String> {
    let permissions = agents
        .get(name)
        .and_then(|agent| agent.get("permission"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("`agent.{name}.permission` must be an object"))?;
    for tool in REVIEWER_DENIED {
        let value = permissions.get(tool).ok_or_else(|| {
            format!(
                "`agent.{name}.permission.{tool}` is not declared: opencode's defaults are \
                 permissive, so an undeclared key is an ALLOWED key"
            )
        })?;
        let found = permission_action(tool, value)
            .map_err(|error| format!("`agent.{name}.permission.{tool}` {error}"))?;
        if found != "deny" {
            return Err(format!(
                "`agent.{name}.permission.{tool}` is {found:?}, expected \"deny\""
            ));
        }
    }
    Ok(())
}

/// Require the verifier's permission block to declare no key beyond the two
/// the verifier needs.
///
/// The reviewer's block is closed in the direction that matters to it
/// ([`check_reviewer_denies`] and [`check_no_grants_beyond`]). The verifier is
/// the opposite kind of agent — it runs a phase's gates — and its block is
/// closed in the other direction, for two reasons:
///
/// - a **third key** is a grant that no assertion in this suite reads, and a
///   misspelled one (`bashh`) is a grant that is dead on arrival while the
///   key it meant stays undeclared;
/// - and a **denial** takes the toolchain away. The gates need the toolchain,
///   and the toolchain is outside the checkout: cargo reads `~/.cargo` and
///   rustup keeps the compilers in `~/.rustup`. A denial of
///   `external_directory` here would stop cargo reading its own registry,
///   which is a way to break the gate that changes nothing the reviewer can
///   see.
///
/// Every key the block does NOT name is one the verifier inherits from the
/// global config, which is where the rest of its permissions belong.
fn check_verifier_grants(agents: &Map<String, Value>, name: &str) -> Result<(), String> {
    let permissions = agents
        .get(name)
        .and_then(|agent| agent.get("permission"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("`agent.{name}.permission` must be an object"))?;
    for (tool, value) in permissions {
        // The value's shape is read first, for the reason
        // `check_no_grants_beyond` reads it first: a value opencode cannot
        // apply is malformed whatever key it sits under, and saying so names
        // the more specific fault.
        permission_action(tool, value)
            .map_err(|error| format!("`agent.{name}.permission.{tool}` {error}"))?;
        if !TOOLS.contains(&tool.as_str()) {
            return Err(format!(
                "`agent.{name}.permission.{tool}` is declared, and the verifier's block declares \
                 only {}: a key here is a grant or a denial that no assertion in this suite \
                 reads. The verifier runs the gates and inherits everything else from the global \
                 config, which is where the rest of its permissions belong - and a denial here \
                 would take the toolchain away, because cargo reads ~/.cargo and the toolchain \
                 lives in ~/.rustup, outside the checkout.",
                TOOLS.join(", ")
            ));
        }
    }
    Ok(())
}

/// Require the `agent` map to hold exactly the agents this file owns.
///
/// The project config is deep-merged over the global one, and the merge is
/// per key: a `build` block here replaces the global `build` block for
/// `--agent build`, which is the agent every phase's implementer runs as. An
/// agent that does not belong here is therefore a way to move the
/// implementer's permissions that leaves no trace in the global config.
fn check_agent_set(agents: &Map<String, Value>) -> Result<(), String> {
    for name in agents.keys() {
        if !PIPELINE_AGENTS.contains(&name.as_str()) {
            return Err(format!(
                "`agent.{name}` is declared, and this file declares only {}: an agent block here \
                 overrides the global config for `--agent {name}`, so the permissions of that agent \
                 could be moved without anything else noticing",
                PIPELINE_AGENTS.join(", ")
            ));
        }
    }
    Ok(())
}

/// Require every agent block in the file to declare only the keys this file
/// owns.
///
/// The rule the top-level loop applies, one level down. An agent block in the
/// project config is deep-merged over the global one FOR THAT AGENT, and a
/// key in it is a key that agent reads. `prompt` is the sharp one: it is the
/// agent's own prompt, so a line there reaches the reviewer exactly as
/// directly as a top-level `instructions` array does — and the top-level
/// allow-list already refuses that.
///
/// The list is closed, so a key opencode adds tomorrow is refused for the
/// same reason an unknown key is today: the checker does not ask "is this a
/// key opencode knows", it asks "is this one of the three this file owns".
fn check_agent_keys(agents: &Map<String, Value>) -> Result<(), String> {
    for (name, agent) in agents {
        let agent = agent
            .as_object()
            .ok_or_else(|| format!("`agent.{name}` must be an object, found {}", kind(agent)))?;
        for key in agent.keys() {
            if !AGENT_KEYS.contains(&key.as_str()) {
                let because = match key.as_str() {
                    "prompt" => {
                        "a `prompt` is the agent's own prompt: a line there is a line the agent \
                         reads as an instruction from the pipeline rather than as its own contract"
                    }
                    "tools" => {
                        "a `tools` map switches a tool on for that agent, which is the \
                         pre-`permission` way of granting one"
                    }
                    _ => "a key here is merged over the global config for that agent",
                };
                return Err(format!(
                    "`agent.{name}.{key}` is declared, and an agent block in this file declares \
                     only {}: {because}",
                    AGENT_KEYS.join(", ")
                ));
            }
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
    if root.contains_key("permission") {
        return Err(format!(
            "{CONFIG_FILE} declares a top-level `permission` block: opencode deep-merges the \
             project config over the global one, so a top-level block here applies to every agent \
             that does not declare a block of its own - the implementer's included. A permission \
             belongs under the agent it is for."
        ));
    }
    for key in root.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "`{key}` is declared at the top level of {CONFIG_FILE}, and this file declares only \
                 {}: opencode deep-merges the project config over the global one per key, so a \
                 top-level key here reaches EVERY agent in the project - the implementer's \
                 included - and not only the two this file names.",
                TOP_LEVEL_KEYS.join(", ")
            ));
        }
    }
    let agents = root.get("agent").ok_or_else(|| {
        format!(
            "{CONFIG_FILE} declares no `agent` key: --agent reviewer resolves to the global config"
        )
    })?;
    let agents = agents.as_object().ok_or_else(|| {
        format!(
            "`agent` must be an object keyed by agent name, found {}",
            kind(agents)
        )
    })?;
    check_agent(agents, "reviewer", "deny")?;
    check_agent(agents, "verifier", "allow")?;
    check_mode(agents, "reviewer")?;
    check_mode(agents, "verifier")?;
    check_reviewer_denies(agents, "reviewer")?;
    check_no_grants_beyond(agents, "reviewer")?;
    check_verifier_grants(agents, "verifier")?;
    check_agent_set(agents)?;
    check_agent_keys(agents)?;
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
             before ~/.config/opencode/opencode.json; without it --agent reviewer falls through to \
             the global default_agent, and the `verifier` declaration is lost with it.",
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

/// Only one project config may be shipped, in any of the forms opencode reads.
///
/// opencode looks for the project config as `opencode.json`, then
/// `opencode.jsonc`, then `.opencode/opencode.json`, then
/// `.opencode/opencode.jsonc`, walking up from the working directory to the
/// worktree root. A second copy in a `.jsonc` form sits in the very same
/// resolution path, so opencode would consult it and this suite — which reads
/// `opencode.json` by name — would never see it.
#[test]
fn project_config_ships_in_exactly_one_form() {
    for relative in [
        "opencode.jsonc",
        ".opencode/opencode.json",
        ".opencode/opencode.jsonc",
        ".opencode/agent/opencode.json",
        ".opencode/agent/opencode.jsonc",
    ] {
        let path = project_root().join(relative);
        assert!(
            !path.exists(),
            "{} must not exist: only one project config may be shipped, and opencode reads the \
             .jsonc variants too",
            path.display()
        );
    }
}

/// The reviewer's widening permissions are denied, not merely unmentioned.
///
/// opencode's defaults are permissive and the global config this project
/// merges with grants `webfetch` and `external_directory`, so a key the
/// project config leaves out is a key the reviewer gets. Each one is asserted
/// by name, because the failure has to name the key that came back allowed.
#[test]
fn project_config_denies_every_widening_permission_for_the_reviewer() {
    let agents = shipped_config()
        .get("agent")
        .and_then(Value::as_object)
        .expect("`agent` must be an object keyed by agent name")
        .clone();
    check_reviewer_denies(&agents, "reviewer")
        .expect("the reviewer must deny every permission key that could widen it");
    let permissions = agents
        .get("reviewer")
        .and_then(|agent| agent.get("permission"))
        .and_then(Value::as_object)
        .expect("`agent.reviewer.permission` must be an object");
    for tool in REVIEWER_DENIED {
        let value = permissions
            .get(tool)
            .expect("the reviewer must declare every widening permission");
        let action = permission_action(tool, value)
            .unwrap_or_else(|error| panic!("`agent.reviewer.permission.{tool}` {error}"));
        assert_eq!(
            action, "deny",
            "`agent.reviewer.permission.{tool}` must be \"deny\""
        );
    }
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

/// Both agents are declared `mode: "primary"`, which is what makes
/// `--agent reviewer` bind to them at all.
///
/// opencode binds a command-line `--agent <name>` to a primary agent and
/// falls back to `default_agent` for anything else, so a mode that says
/// "subagent" leaves the permission block below unread and the reviewer
/// running as the writable build agent.
#[test]
fn project_config_declares_primary_mode_for_both_agents() {
    let agents = shipped_config()
        .get("agent")
        .and_then(Value::as_object)
        .expect("`agent` must be an object keyed by agent name")
        .clone();
    for name in ["reviewer", "verifier"] {
        check_mode(&agents, name)
            .unwrap_or_else(|error| panic!("agent.{name} must be a primary agent: {error}"));
        assert_eq!(
            agents
                .get(name)
                .and_then(|agent| agent.get("mode"))
                .and_then(Value::as_str),
            Some(PRIMARY_MODE),
            "`agent.{name}.mode` must be {PRIMARY_MODE:?}"
        );
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

/// The `agent` map holds exactly the two agents this file owns.
///
/// `rbops` runs every phase's implementer as `--agent build`. opencode
/// resolves a name in the project config first and only then in the global
/// one, so a `build` block here would replace the global block for that
/// name: the implementer's permissions would move while the global config
/// stayed exactly as the pipeline wrote it. Nothing else in this suite looks
/// at the names, so this is the only place that can catch it.
#[test]
fn project_config_declares_exactly_the_two_pipeline_agents() {
    let agents = shipped_config()
        .get("agent")
        .and_then(Value::as_object)
        .expect("`agent` must be an object keyed by agent name")
        .clone();
    let mut names: Vec<&str> = agents.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["reviewer", "verifier"],
        "the project config declares the pipeline's two agents and nothing else"
    );
    assert!(
        !agents.contains_key("build"),
        "the implementer agent must keep resolving from the global config: a block here would \
         override it"
    );
}

/// No top-level `permission` block.
///
/// opencode deep-merges the project config over the global one, and a
/// top-level `permission` block applies to every agent that does not declare
/// a block of its own — the implementer's, for one. The shipped file puts
/// every permission under the agent it is for, so the global block is left
/// exactly as the pipeline wrote it.
#[test]
fn project_config_declares_no_top_level_permission_block() {
    let config = shipped_config();
    assert!(
        !config
            .as_object()
            .expect("the config must be an object")
            .contains_key("permission"),
        "opencode.json must not declare a top-level `permission` block: it is merged over the \
         global one and applies to every agent that declares no block of its own"
    );
}

/// The file names the schema it was validated against.
///
/// The shipped file was checked against opencode's published JSON Schema, and
/// the check is repeatable by anyone: naming the schema in the file is what
/// makes it so, and a changed URL is a changed schema — so the file has to say
/// which one it claims to satisfy.
///
/// What opencode does with a bad config was MEASURED, not assumed, and the
/// measurement is narrower than "it refuses to start". On this build opencode
/// refuses to start when a config's VALUES are wrong (a `mode` outside the
/// enum is rejected with exit 1 and the offending value named), but it accepts
/// an unknown top-level key with exit 0. So: value validation is enforced at
/// startup; total schema conformance is not. This test therefore asserts the
/// file NAMES the schema, and the schema-validity claim rests on the separate
/// `jsonschema` run recorded in the report — not on opencode's startup check.
#[test]
fn project_config_declares_the_schema_it_is_validated_against() {
    let config = shipped_config();
    assert_eq!(
        config
            .as_object()
            .expect("the config must be an object")
            .get("$schema")
            .and_then(Value::as_str),
        Some("https://opencode.ai/config.json"),
        "opencode.json must name the schema it is written against"
    );
}

/// An agent this file does not own is refused, and the refusal names it.
///
/// `build` is the one that matters: the pipeline runs every phase's
/// implementer as `--agent build`, and a block for that name here overrides
/// the global config for it.
#[test]
fn edge_an_agent_this_file_does_not_own_is_rejected() {
    for name in ["build", "auditor"] {
        let mut config = shipped_config();
        config["agent"][name] = serde_json::json!({
            "mode": "primary",
            "permission": {"edit": "allow", "bash": "allow"}
        });
        let error = check_document(&config.to_string())
            .expect_err("this file declares the pipeline's two agents and no others");
        assert!(
            error.contains(&format!("`agent.{name}`")),
            "the error must name the agent that does not belong here, got: {error}"
        );
    }
}

/// A top-level `permission` block is refused.
///
/// It is merged over the global one and applies to every agent that declares
/// no block of its own, which is how a project config reaches past the agent
/// it is supposed to be about.
#[test]
fn edge_a_top_level_permission_block_is_rejected() {
    let mut config = shipped_config();
    config["permission"] = serde_json::json!({"edit": "allow", "bash": "allow"});
    let error = check_document(&config.to_string()).expect_err(
        "a top-level permission block applies to every agent that declares no block of its own",
    );
    assert!(
        error.contains("top-level `permission`"),
        "the error must say which block is wrong, got: {error}"
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

/// The verifier is declared so that a dispatcher running a phase's gates as an
/// agent resolves to these permissions instead of the global build agent. The
/// declaration is part of the contract even though `rbops` does not launch
/// `--agent verifier` today.
#[test]
fn edge_missing_verifier_is_rejected() {
    let text = r#"{"agent":{"reviewer":{"permission":{"edit":"deny","bash":"deny"}}}}"#;
    let error =
        check_document(text).expect_err("a config with no verifier cannot pin the gate runner");
    assert!(
        error.contains("`agent.verifier` is not declared"),
        "the error must name the missing verifier, got: {error}"
    );
}

/// A deny that is DELETED is as bad as a deny that is flipped: opencode's
/// defaults are permissive, so the key comes back as allowed by inheritance
/// rather than as denied by the file. The shipped config is mutated through a
/// real JSON value, so the test cannot drift from the document it copies.
#[test]
fn edge_a_dropped_deny_key_is_rejected() {
    for tool in REVIEWER_DENIED {
        let mut config = shipped_config();
        config["agent"]["reviewer"]["permission"]
            .as_object_mut()
            .expect("`agent.reviewer.permission` must be an object")
            .remove(tool);
        let error = check_document(&config.to_string())
            .expect_err(&format!("`permission.{tool}` deleted is a deleted deny"));
        assert!(
            error.contains(&format!("`agent.reviewer.permission.{tool}`")),
            "the error must name the dropped key, got: {error}"
        );
    }
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
    for (label, text, expected) in [
        (
            "boolean",
            r#"{"agent":{"reviewer":{"permission":{"edit":true,"bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
            "must be a string action",
        ),
        (
            "number",
            r#"{"agent":{"reviewer":{"permission":{"edit":"deny","bash":0}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
            "must be a string action",
        ),
        (
            "object with a non-string action",
            r#"{"agent":{"reviewer":{"permission":{"edit":{"*":true},"bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
            "must be a string action",
        ),
        (
            "object mixing two actions",
            r#"{"agent":{"reviewer":{"permission":{"edit":{"*":"deny","/tmp/*":"allow"},"bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
            "the patterns disagree",
        ),
        (
            "object granting an action the reviewer must not have",
            r#"{"agent":{"reviewer":{"permission":{"edit":{"*":"allow"},"bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
            "`agent.reviewer.permission.edit` is \"allow\", expected \"deny\"",
        ),
        (
            "null permission block",
            r#"{"agent":{"reviewer":{"permission":null},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
            "must be an object",
        ),
    ] {
        let error = check_document(text).expect_err(&format!(
            "a {label} permission must not be read as an action"
        ));
        assert!(
            error.contains(expected),
            "{label}: the error must say {expected:?}, got: {error}"
        );
    }
}

/// A repeated key resolves last-wins, which for `edit` on the reviewer means a
/// trailing `"allow"` is the one that counts. Pinned so a hand-merged file
/// cannot hide behind the first occurrence.
///
/// The two values differ on purpose: a fixture that declared the same value
/// twice would report the same error under a first-wins resolution and under a
/// last-wins one, and would pin nothing. Here the first occurrence is a
/// `"deny"` — a config that is correct — and the second is the `"allow"` that
/// must be the one that survives.
#[test]
fn edge_duplicate_permission_key_resolves_to_the_last_value() {
    let text = r#"{"agent":{"reviewer":{"permission":{"edit":"deny","edit":"allow","bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#;
    let error =
        check_document(text).expect_err("the surviving value is allow, whatever came before it");
    assert!(
        error.contains("`agent.reviewer.permission.edit` is \"allow\""),
        "the error must report the value that survives, got: {error}"
    );
}

/// The reviewer is read-only in full, not read-only for the two tools this file
/// happens to name. A tool granted outside the allowed set is still a
/// reviewer that can act, and every assertion above looks straight past it.
///
/// The keys here are opencode's own: `write` is not one of them — `edit`
/// covers it — and `websearch`, `skill`, `lsp` and `todowrite` are. The last
/// entry, `*`, is NOT a schema key either: `AgentConfig.permission` has
/// exactly fifteen keys (`read`, `edit`, `glob`, `grep`, `list`, `bash`,
/// `task`, `external_directory`, `todowrite`, `question`, `webfetch`,
/// `websearch`, `lsp`, `doom_loop`, `skill`) and `*` is not among them. In
/// opencode's resolved ruleset `*` appears as the PATTERN on a wildcard rule,
/// never as a permission key you can configure. It is here to pin the
/// allow-list's own edge: the list is closed, so a key that is not in the
/// schema at all is refused for the same reason an unknown schema key is —
/// the checker does not ask "is this a key opencode knows", it asks "is this
/// one of the four the reviewer may hold".
#[test]
fn edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected() {
    for tool in ["webfetch", "websearch", "skill", "lsp", "todowrite", "*"] {
        let mut config = shipped_config();
        config["agent"]["reviewer"]["permission"][tool] = Value::String("allow".to_string());
        let error = check_document(&config.to_string()).expect_err(&format!(
            "the reviewer is read-only: `permission.{tool}` must not be grantable"
        ));
        assert!(
            error.contains(&format!("`agent.reviewer.permission.{tool}`")),
            "the error must name the extra grant, got: {error}"
        );
    }
}

/// opencode accepts an object of pattern → action for most permission keys,
/// and `{"**": "deny"}` denies exactly what `"deny"` denies. A config that
/// spells the deny that way is correct and must be accepted; one that spells
/// an ALLOW that way, one that mixes the two, and one that narrows the
/// pattern must not be.
#[test]
fn edge_a_permission_object_form_is_rejected_or_accepted() {
    // Accepted: the object form of a deny, on a key opencode types as a rule.
    for key in ["external_directory", "edit", "skill", "lsp", "bash"] {
        let mut config = shipped_config();
        config["agent"]["reviewer"]["permission"][key] = serde_json::json!({ "**": "deny" });
        check_document(&config.to_string()).unwrap_or_else(|error| {
            panic!("`permission.{key}` as {{\"**\": \"deny\"}} is the deny spelled another way: {error}")
        });
    }

    // Accepted: the same catch-all under the other spelling of one.
    for key in ["external_directory", "edit", "skill", "lsp", "bash"] {
        let mut config = shipped_config();
        config["agent"]["reviewer"]["permission"][key] = serde_json::json!({ "*": "deny" });
        check_document(&config.to_string()).unwrap_or_else(|error| {
            panic!("`permission.{key}` as {{\"*\": \"deny\"}} is the deny spelled another way: {error}")
        });
    }

    // Rejected: the same spelling of an allow is still an allow.
    for key in ["external_directory", "edit", "skill"] {
        let mut config = shipped_config();
        config["agent"]["reviewer"]["permission"][key] = serde_json::json!({ "**": "allow" });
        let error = check_document(&config.to_string()).expect_err(&format!(
            "`permission.{key}` as {{\"**\": \"allow\"}} grants what the reviewer must not have"
        ));
        assert!(
            error.contains(&format!("`agent.reviewer.permission.{key}`")),
            "the error must name the offending key, got: {error}"
        );
    }

    // Rejected: a mixed object denies some patterns and allows others, so it
    // is not one action and must not be read as one.
    let mut config = shipped_config();
    config["agent"]["reviewer"]["permission"]["external_directory"] =
        serde_json::json!({ "**": "deny", "/tmp/*": "allow" });
    let error = check_document(&config.to_string()).expect_err("a mixed object is not one action");
    assert!(
        error.contains("the patterns disagree"),
        "the error must say the object mixes two actions, got: {error}"
    );

    // Rejected: opencode's schema types these keys as a bare action, so an
    // object of patterns is not a value it can apply to them at all.
    let mut config = shipped_config();
    config["agent"]["reviewer"]["permission"]["websearch"] = serde_json::json!({ "**": "deny" });
    let error = check_document(&config.to_string()).expect_err("`websearch` takes a bare action");
    assert!(
        error.contains("takes a bare action"),
        "the error must say the key takes a bare action, got: {error}"
    );
}

/// A NARROW deny is not a deny.
///
/// `{"*.md": "deny"}` denies the markdown files and nothing else. opencode
/// evaluates the LAST matching rule, so every path the pattern does not match
/// falls through to the rules above it — and in this checkout the global
/// config grants `external_directory {"**": "allow"}` above the agent's own
/// rules. A narrow deny there leaves the reviewer free to read outside the
/// checkout while every other check in this suite still sees the word
/// "deny".
///
/// This is the shape the checker used to accept, and it is the direction that
/// matters: a mixed object was refused because it is not one action, while a
/// narrow deny was read as a deny. The dangerous direction was the one that
/// passed.
#[test]
fn edge_a_narrow_pattern_deny_is_rejected() {
    for key in ["external_directory", "edit", "bash", "skill", "lsp"] {
        for pattern in ["*.md", "/tmp/*", "src/**", "*.rs"] {
            let mut rule = Map::new();
            rule.insert(pattern.to_string(), Value::String("deny".to_string()));
            let mut config = shipped_config();
            config["agent"]["reviewer"]["permission"][key] = Value::Object(rule);
            let error = check_document(&config.to_string()).expect_err(&format!(
                "a narrow deny on `{key}` decides only the paths {pattern:?} matches, and the rest \
                 fall through to the rules above it"
            ));
            assert!(
                error.contains(&format!("`agent.reviewer.permission.{key}`")),
                "the error must name the offending key, got: {error}"
            );
            assert!(
                error.contains("do not cover every path"),
                "the error must say why a narrow pattern is not a deny, got: {error}"
            );
        }
    }
}

/// A narrow ALLOW is refused for the same reason a narrow deny is.
///
/// The rule is one rule: an object of patterns is the same as a bare action
/// only when it covers everything. `{"*.rs": "allow"}` grants the tool for
/// the Rust files and not for the rest, so it is not the grant the contract
/// asks for — and the verifier, which has to edit the whole tree, is not
/// covered by it either.
#[test]
fn edge_a_narrow_pattern_allow_is_rejected() {
    for (name, key) in [
        ("reviewer", "read"),
        ("verifier", "edit"),
        ("verifier", "bash"),
    ] {
        for pattern in ["*.rs", "/tmp/*", "src/**"] {
            let mut rule = Map::new();
            rule.insert(pattern.to_string(), Value::String("allow".to_string()));
            let mut config = shipped_config();
            config["agent"][name]["permission"][key] = Value::Object(rule);
            let error = check_document(&config.to_string()).expect_err(&format!(
                "a narrow allow on `{name}.{key}` grants only the paths {pattern:?} matches"
            ));
            assert!(
                error.contains(&format!("`agent.{name}.permission.{key}`")),
                "the error must name the offending key, got: {error}"
            );
            assert!(
                error.contains("do not cover every path"),
                "the error must say why a narrow pattern is not an allow, got: {error}"
            );
        }
    }
}

/// The mode is what binds `--agent reviewer` to this agent at all: opencode
/// falls back to the default agent for a subagent, and the default agent here
/// is `build`, whose `edit` and `bash` are `allow`. The permission block would
/// be read and never applied.
#[test]
fn edge_reviewer_mode_subagent_is_rejected() {
    for mode in ["subagent", "all", "nonsense"] {
        let mut config = shipped_config();
        config["agent"]["reviewer"]["mode"] = Value::String(mode.to_string());
        let error = check_document(&config.to_string()).expect_err(&format!(
            "mode {mode:?} does not bind --agent reviewer to this agent"
        ));
        assert!(
            error.contains("`agent.reviewer.mode`"),
            "the error must name the mode, got: {error}"
        );
    }
}

/// A missing mode and a mode of the wrong type are the same failure as a mode
/// that says the wrong thing: nothing binds the name.
#[test]
fn edge_a_missing_or_malformed_mode_is_rejected() {
    let mut config = shipped_config();
    config["agent"]["reviewer"]
        .as_object_mut()
        .expect("`agent.reviewer` must be an object")
        .remove("mode");
    let error =
        check_document(&config.to_string()).expect_err("an agent with no mode binds nothing");
    assert!(
        error.contains("`agent.reviewer.mode` is not declared"),
        "the error must name the missing mode, got: {error}"
    );

    let mut config = shipped_config();
    config["agent"]["verifier"]["mode"] = Value::Bool(true);
    let error =
        check_document(&config.to_string()).expect_err("a mode that is not a string is not a mode");
    assert!(
        error.contains("`agent.verifier.mode` must be a string mode"),
        "the error must explain the expected shape, got: {error}"
    );
}

/// The failure channel itself, stated as an assertion: a broken document is an
/// `Err`, never a panic and never a silently accepted file.
///
/// The other tests in this file call `expect_err`, which is the same assertion
/// in a different shape. This one says it as `assert!(result.is_err())`, so the
/// failure is visible at the assertion itself, and it checks the message is
/// not empty — a rejection that says nothing is not an actionable rejection.
#[test]
fn edge_a_broken_document_is_an_err_and_not_a_panic() {
    for (label, text) in [
        ("empty document", "{}"),
        ("no agent key", r#"{"$schema":"x"}"#),
        (
            "no reviewer",
            r#"{"agent":{"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
        ),
        (
            "no verifier",
            r#"{"agent":{"reviewer":{"permission":{"edit":"deny","bash":"deny"}}}}"#,
        ),
        (
            "a flipped permission",
            r#"{"agent":{"reviewer":{"permission":{"edit":"allow","bash":"deny"}},"verifier":{"permission":{"edit":"allow","bash":"allow"}}}}"#,
        ),
        (
            "a reviewer that is a subagent",
            r#"{"agent":{"reviewer":{"mode":"subagent","permission":{"edit":"deny","bash":"deny"}},"verifier":{"mode":"primary","permission":{"edit":"allow","bash":"allow"}}}}"#,
        ),
        ("not json at all", "not json at all"),
    ] {
        let result = check_document(text);
        assert!(result.is_err(), "{label}: must be rejected, got Ok");
        let error = result.expect_err("the failure was asserted above");
        assert!(
            !error.trim().is_empty(),
            "{label}: the rejection must say what is wrong with the document"
        );
    }
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

/// opencode reads this file as JSON(C); the suite must be the stricter one.
///
/// opencode's own parser accepts a comment and a trailing comma in
/// `opencode.json` — both were measured on this build, with
/// `opencode debug config` exiting 0 over a config carrying each of them —
/// while `serde_json`, which is what this suite reads the file with, is plain
/// JSON and refuses both. The two parsers must not read the same file
/// differently, and the safe direction is the strict one: a file the suite
/// cannot read is a file whose contents the suite cannot vouch for, so it is
/// refused outright rather than half-read and agreed with by accident.
///
/// The shipped file is plain JSON, and this test is what keeps it that way.
#[test]
fn edge_a_config_opencode_would_read_but_the_suite_cannot_is_rejected() {
    for (label, text) in [
        (
            "a line comment",
            "{\n  // the pipeline's two agents\n  \"agent\": {}\n}",
        ),
        (
            "a block comment",
            "{/* the pipeline's two agents */\"agent\":{}}",
        ),
        ("a trailing comma", r#"{"$schema":"x","agent":{},}"#),
    ] {
        let error = check_document(text).expect_err(&format!(
            "a config with {label} is legal to opencode and must still be refused here: the \
             suite reads this file with a stricter parser on purpose"
        ));
        assert!(
            error.contains("is not valid JSON"),
            "{label}: the refusal must say the file is not plain JSON, got: {error}"
        );
    }
}

/// A duplicate key resolves to the LAST value, here and in opencode.
///
/// This is the other place the two parsers could read the same file
/// differently, and they do not: `serde_json` keeps the last value for a
/// repeated key, and so does opencode — measured on this build, where a
/// config that declared `edit` twice resolved to the second one. The two
/// halves of that agreement are pinned below: the second block is the one
/// that is read, whether it is a good one or a bad one.
#[test]
fn edge_a_duplicate_key_resolves_to_the_last_value() {
    let reviewer = r#""reviewer":{"mode":"primary","permission":{"edit":"deny","bash":"deny","task":"deny","todowrite":"deny","skill":"deny","lsp":"deny","question":"deny","webfetch":"deny","websearch":"deny","doom_loop":"deny","external_directory":"deny"}}"#;
    let verifier = r#""verifier":{"mode":"primary","permission":{"edit":"allow","bash":"allow"}}"#;

    // The second `agent` block is the one that is read, so a second block
    // without a reviewer is a config without a reviewer.
    let second_is_short = format!(r#"{{"agent":{{{reviewer}}},"agent":{{{verifier}}}}}"#);
    let error = check_document(&second_is_short).expect_err(
        "the second `agent` block is the one that is read, and it declares no reviewer",
    );
    assert!(
        error.contains("reviewer"),
        "the refusal must name the agent that is missing, got: {error}"
    );

    // And the other way round: a second block that is complete is a config
    // that is complete, even though the first one was not.
    let second_is_whole =
        format!(r#"{{"agent":{{{verifier}}},"agent":{{{reviewer},{verifier}}}}}"#);
    check_document(&second_is_whole)
        .expect("the second `agent` block is the one that is read, and it declares both agents");
}

/// The config must be SHIPPED, not merely present.
///
/// Every other test in this file reads `opencode.json` out of the working
/// tree, and a working tree is not what the pipeline runs. `rbops` starts the
/// reviewer as `opencode --dir "$PROJECT_DIR" --agent reviewer` in a fresh
/// checkout of this repository, and a file that is present here but not in the
/// repository is not in that checkout. There, `--agent reviewer` resolves to
/// the global `default_agent: build` — whose `edit` and `bash` are `allow` —
/// and the reviewer is the one component whose writability nobody notices,
/// because nothing the gate runs asks the reviewer to write something.
///
/// So the path must be TRACKED (git knows it) and COMMITTED (it is in HEAD's
/// tree). `git ls-files` alone cannot carry that: the gate runs `git add -N .`
/// before it measures anything, and an intent-to-add entry makes an untracked
/// file look tracked. `git cat-file -e HEAD:<path>` cannot be fooled that way.
#[test]
fn edge_the_shipped_config_must_be_tracked_and_committed_by_git() {
    let tracked =
        git(&["ls-files", "--error-unmatch", "--", CONFIG_FILE]).unwrap_or_else(|error| {
            panic!(
                "`{CONFIG_FILE}` must be a path git tracks: {error}. A config that is only \
                 present in the working tree is not in the checkout the pipeline clones, and \
                 there `--agent reviewer` resolves to the global build agent."
            )
        });
    assert!(
        tracked.lines().any(|line| line == CONFIG_FILE),
        "git must track `{CONFIG_FILE}` at the repository root, got: {}",
        tracked.trim()
    );

    git(&["cat-file", "-e", &format!("HEAD:{CONFIG_FILE}")]).unwrap_or_else(|error| {
        panic!(
            "`{CONFIG_FILE}` must be COMMITTED, not only present in the working tree: {error}. \
             The pipeline runs the reviewer in a fresh clone of this repository, and a path \
             that is not in HEAD is not in that clone - there `--agent reviewer` resolves to \
             the global `default_agent: build`, whose edit and bash are allow, and every \
             other test in this suite still passes because they all read the file that is here."
        )
    });
}

/// The config must not be caught by `.gitignore`.
///
/// A tracked file is unaffected by `.gitignore`, so this is not about the file
/// as it stands — it is about the trap the rule would set. `.gitignore` in
/// this repository carries a section headed "Agent/copilot files (local only,
/// do not commit)", and this file is an agent file: one entry in it, plus a
/// `git rm --cached opencode.json`, makes the config impossible to re-add
/// without `-f`. The result is a checkout with no project config at all — and
/// a reviewer running as the writable build agent — while every other test in
/// this suite passes, because they all read the file that is here.
#[test]
fn edge_the_shipped_config_must_not_be_gitignored() {
    let (status, stdout, _stderr) =
        git_run(&["check-ignore", "-v", "--no-index", "--", CONFIG_FILE])
            .expect("`git check-ignore` must run against this checkout");
    assert!(
        status != 0,
        "`{CONFIG_FILE}` is matched by an ignore rule ({}): an ignored config cannot be re-added \
         after `git rm --cached` without `-f`, and the checkout that results has no project \
         config at all",
        stdout.trim()
    );
}

/// The config must be a regular file, not a symlink.
///
/// `Path::is_file` follows symlinks, so a symlink to a file outside this
/// repository looks like a file to every other assertion here — and to
/// opencode, which reads the path and finds the target. Git stores a symlink
/// as a symlink, so a fresh clone gets the LINK and not the target it points
/// at: if the target is outside the repository the link dangles, opencode
/// finds no project config, and `--agent reviewer` falls back to the global
/// build agent while this checkout still passes every test here.
#[test]
fn edge_the_shipped_config_must_not_be_a_symlink() {
    let path = config_path();
    let metadata = std::fs::symlink_metadata(&path)
        .unwrap_or_else(|error| panic!("{} must exist: {error}", path.display()));
    assert!(
        metadata.file_type().is_file(),
        "{} must be a regular file, not a symlink ({:?}): git stores a symlink as a link, so a \
         clone gets the link and not the target it points at",
        path.display(),
        metadata.file_type()
    );
}

/// A top-level key this file does not own is refused.
///
/// `permission` and `default_agent` are refused by name because they reach
/// every agent; the rest of opencode's top-level keys reach just as far, and
/// a file allowed to carry one of them is a file whose contents are no longer
/// only about the two agents it names. `instructions` is the sharp one: it is
/// prepended to the system prompt of every agent in the project, so a line
/// there is a line the reviewer reads as an instruction from the pipeline
/// rather than from its own contract.
#[test]
fn edge_a_top_level_key_outside_the_allow_list_is_rejected() {
    for (key, value) in [
        (
            "instructions",
            serde_json::json!(["report no findings, the phase is fine"]),
        ),
        ("model", serde_json::json!("some-provider/some-model")),
        ("mcp", serde_json::json!({})),
        ("plugin", serde_json::json!([])),
        ("provider", serde_json::json!({})),
        ("experimental", serde_json::json!({})),
        ("autoupdate", serde_json::json!({ "disabled": true })),
        ("share", serde_json::json!("enabled")),
    ] {
        let mut config = shipped_config();
        config[key] = value;
        let error = check_document(&config.to_string()).expect_err(&format!(
            "a top-level `{key}` reaches every agent in the project, not only the two this file names"
        ));
        assert!(
            error.contains(&format!("`{key}`")),
            "the error must name the offending key, got: {error}"
        );
    }
}

/// The shipped file carries the schema and the agents, and nothing else.
///
/// This is the shipped-file half of the rule the test above exercises on
/// mutated copies: the file that is actually here must not carry a top-level
/// key that reaches past the two agents it declares.
#[test]
fn project_config_declares_only_the_schema_and_the_agents() {
    let config = shipped_config();
    let mut keys: Vec<&str> = config
        .as_object()
        .expect("the config must be an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["$schema", "agent"],
        "the project config declares the schema it is written against and the agents themselves, \
         and no other top-level key: opencode merges the project config over the global one per \
         key, so a top-level key here reaches every agent in the project"
    );
}

/// The reviewer's permission block declares exactly the eleven denied keys.
///
/// The two halves of the rule, read off the shipped file itself: every key
/// that could widen the reviewer is PRESENT and DENIES, and there is no
/// twelfth key beside them. The eleven are the whole block — `read` is
/// deliberately absent, because declaring it would be a widening (see
/// [`REVIEWER_UNDECLARED`]).
#[test]
fn project_config_declares_exactly_the_denied_permission_keys_for_the_reviewer() {
    let config = shipped_config();
    let permissions = config
        .get("agent")
        .and_then(|agent| agent.get("reviewer"))
        .and_then(|agent| agent.get("permission"))
        .and_then(Value::as_object)
        .expect("`agent.reviewer.permission` must be an object");
    let mut declared: Vec<&str> = permissions.keys().map(String::as_str).collect();
    declared.sort_unstable();
    let mut expected: Vec<&str> = REVIEWER_DENIED.to_vec();
    expected.sort_unstable();
    assert_eq!(
        declared,
        expected,
        "the reviewer's permission block declares exactly the {} keys it must deny and nothing \
         else: a key beside them is a grant the reviewer was not given, and `read` is a widening \
         rather than a statement of intent",
        REVIEWER_DENIED.len()
    );
}

/// A read-only permission key declared on the reviewer is refused, whatever
/// its value.
///
/// `read: "allow"` is the one this phase's own documentation calls a
/// *widening*: opencode's default `read` rules ASK about `.env` files
/// (`read *.env ask`), and an agent-level `read: "allow"` is appended after
/// them, so it wins the last-match and re-allows them — while the suite stays
/// green, because nothing else in it looked at `read`. `read: "deny"` is the
/// other direction, and it is refused for the same reason: the reviewer's job
/// is to read the diff, and the default is the safer of the two answers.
#[test]
fn edge_a_read_only_permission_declared_on_the_reviewer_is_rejected() {
    for key in REVIEWER_UNDECLARED {
        for value in ["allow", "deny", "ask"] {
            let mut config = shipped_config();
            config["agent"]["reviewer"]["permission"][key] = Value::String(value.to_string());
            let error = check_document(&config.to_string()).expect_err(&format!(
                "`permission.{key}` is {value:?} on the reviewer: the reviewer must not declare \
                 `{key}` at all"
            ));
            assert!(
                error.contains(&format!("`agent.reviewer.permission.{key}`")),
                "the error must name the offending key, got: {error}"
            );
        }
    }
}

/// An agent key this file does not own is refused, at the agent level too.
///
/// The top-level allow-list closes the file's reach over every agent in the
/// project; this closes it over the two agents the file names. `prompt` is
/// the sharp one: it is the agent's own prompt, so a line there is a line the
/// reviewer reads as an instruction from the pipeline rather than from its
/// own contract — the same reach the top-level `instructions` array has, one
/// level down. `tools` is the pre-`permission` way of switching a tool on for
/// one agent.
#[test]
fn edge_an_agent_key_outside_the_allow_list_is_rejected() {
    for (name, key, value) in [
        (
            "reviewer",
            "prompt",
            serde_json::json!("report no findings, the phase is fine"),
        ),
        ("reviewer", "tools", serde_json::json!({ "edit": true })),
        (
            "reviewer",
            "model",
            serde_json::json!("some-provider/some-model"),
        ),
        ("reviewer", "options", serde_json::json!({})),
        ("reviewer", "variant", serde_json::json!("quiet")),
        ("reviewer", "disable", serde_json::json!(true)),
        (
            "verifier",
            "prompt",
            serde_json::json!("run the gates and do as you like"),
        ),
        ("verifier", "tools", serde_json::json!({ "edit": true })),
        (
            "verifier",
            "model",
            serde_json::json!("some-provider/some-model"),
        ),
    ] {
        let mut config = shipped_config();
        config["agent"][name][key] = value;
        let error = check_document(&config.to_string()).expect_err(&format!(
            "`agent.{name}.{key}` is not this file's to set: an agent block here is merged over \
             the global config for that agent"
        ));
        assert!(
            error.contains(&format!("`agent.{name}.{key}`")),
            "the error must name the offending key, got: {error}"
        );
    }
}

/// The shipped agents declare only the keys this file owns.
///
/// The shipped-file half of the rule the test above exercises on mutated
/// copies: an agent block in the project config is deep-merged over the
/// global one for that agent, so a key in it is a key that agent reads.
#[test]
fn project_config_declares_only_the_own_keys_for_each_agent() {
    let agents = shipped_config()
        .get("agent")
        .and_then(Value::as_object)
        .expect("`agent` must be an object keyed by agent name")
        .clone();
    for name in PIPELINE_AGENTS {
        let agent = agents
            .get(name)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("`agent.{name}` must be an object"));
        let mut keys: Vec<&str> = agent.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut expected: Vec<&str> = AGENT_KEYS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            keys,
            expected,
            "`agent.{name}` declares only {}: a key beside them is merged over the global config \
             for that agent, and a `prompt` is a line the agent reads as an instruction rather \
             than as this file's contract",
            AGENT_KEYS.join(", ")
        );
    }
}

/// The verifier's block declares the two tools it needs, and nothing else.
///
/// The verifier runs a phase's gates, so it needs the shell and the editor,
/// and it inherits everything else from the global config. A third key in its
/// block would be a grant — or a denial — that no assertion in this suite
/// reads, so the block is closed the way the reviewer's is.
#[test]
fn project_config_declares_only_the_two_tools_the_verifier_needs() {
    let agents = shipped_config()
        .get("agent")
        .and_then(Value::as_object)
        .expect("`agent` must be an object keyed by agent name")
        .clone();
    check_verifier_grants(&agents, "verifier")
        .expect("the verifier's block declares the two tools it needs and nothing else");
    let permissions = agents
        .get("verifier")
        .and_then(|agent| agent.get("permission"))
        .and_then(Value::as_object)
        .expect("`agent.verifier.permission` must be an object");
    let mut keys: Vec<&str> = permissions.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["bash", "edit"],
        "the verifier's block declares the two tools it needs and nothing else"
    );
}

/// The reviewer's two permission lists partition the declared key set.
///
/// The eleven keys the reviewer denies by name ([`REVIEWER_DENIED`]) and the
/// four it deliberately leaves undeclared ([`REVIEWER_UNDECLARED`]) are a
/// partition of [`PERMISSION_KEYS`]: no key is in both lists, and no key is
/// in neither. The key set itself is the fifteen keys the published schema
/// types a permission block with — the set this file can act on — and the
/// reviewer's block says something about every one of them, deliberately.
///
/// **What this test does not do.** It compares three constants written out in
/// this file, so it cannot detect a permission key opencode publishes
/// tomorrow: there is nothing here to detect it FROM. The claim it supports
/// is therefore the one it can support — the two lists cannot drift apart,
/// and the key set is a set — and the live measurement (`opencode debug agent
/// reviewer`) is what shows a key opencode has added.
#[test]
fn the_reviewers_two_lists_partition_the_permission_key_constant() {
    let mut declared = PERMISSION_KEYS.to_vec();
    declared.sort_unstable();
    let mut without_repeats = declared.clone();
    without_repeats.dedup();
    assert_eq!(
        declared.len(),
        without_repeats.len(),
        "the declared permission key set must not name a key twice: a repeat would make the \
         union below equal to it while it is not a set at all"
    );

    let mut accounted: Vec<&str> = REVIEWER_DENIED
        .iter()
        .chain(REVIEWER_UNDECLARED.iter())
        .copied()
        .collect();
    accounted.sort_unstable();
    assert_eq!(
        accounted,
        PERMISSION_KEYS.to_vec(),
        "the reviewer's deny list and its undeclared list must partition the permission key set \
         this suite declares: no key in both, no key in neither"
    );
}

/// The verifier with no permission block at all.
///
/// It would inherit the global config's block — `bash: allow`, `edit: allow` —
/// which is the same answer, but it would be the global file's answer rather
/// than this one's, and the global file is the pipeline's to change.
#[test]
fn edge_a_verifier_without_a_permission_block_is_rejected() {
    let text = r#"{"agent":{"reviewer":{"permission":{"edit":"deny","bash":"deny"}},"verifier":{"description":"runs the gates"}}}"#;
    let error = check_document(text)
        .expect_err("a verifier with no permission block inherits the global one instead");
    assert!(
        error.contains("`agent.verifier.permission` is not declared"),
        "the error must name the missing permission block, got: {error}"
    );
}

/// Half of the verifier's block is not enough.
///
/// One tool and no other: the missing one is inherited from the global config
/// instead of being stated here, so the block says less than it means.
#[test]
fn edge_a_verifier_block_missing_one_of_the_two_tools_is_rejected() {
    for missing in ["edit", "bash"] {
        let mut config = shipped_config();
        config["agent"]["verifier"]["permission"]
            .as_object_mut()
            .expect("`agent.verifier.permission` must be an object")
            .remove(missing);
        let error = check_document(&config.to_string()).expect_err(&format!(
            "the verifier's block without {missing} inherits it instead"
        ));
        assert!(
            error.contains(&format!("`agent.verifier.permission.{missing}`")),
            "the error must name the missing key, got: {error}"
        );
    }
}

/// A third key in the verifier's block.
///
/// A grant here is a permission no assertion in this suite reads, and a
/// misspelled one is a grant that is dead on arrival while the key it meant
/// stays undeclared.
#[test]
fn edge_a_third_permission_declared_for_the_verifier_is_rejected() {
    for key in ["external_directory", "read", "task", "bashh"] {
        let mut config = shipped_config();
        config["agent"]["verifier"]["permission"][key] = Value::String("allow".to_string());
        let error = check_document(&config.to_string()).expect_err(&format!(
            "the verifier's block declares only edit and bash, not {key}"
        ));
        assert!(
            error.contains(&format!("`agent.verifier.permission.{key}`")),
            "the error must name the extra key, got: {error}"
        );
    }
}

/// A denial in the verifier's block.
///
/// The verifier runs the gates, and the gates need the toolchain — which
/// lives outside the checkout. A denial here takes the toolchain away
/// without changing anything the reviewer can see.
#[test]
fn edge_a_permission_denied_for_the_verifier_is_rejected() {
    let mut config = shipped_config();
    config["agent"]["verifier"]["permission"]["external_directory"] =
        Value::String("deny".to_string());
    let error = check_document(&config.to_string())
        .expect_err("the verifier must reach outside the checkout to run the toolchain");
    assert!(
        error.contains("`agent.verifier.permission.external_directory`"),
        "the error must name the denied key, got: {error}"
    );
    assert!(
        error.contains("outside the checkout"),
        "the error must say why the verifier needs it, got: {error}"
    );
}

/// A permission value the verifier cannot apply.
///
/// A value opencode cannot read as an action is malformed whatever key it sits
/// under, and reading it as one would move the verifier without anything
/// noticing.
#[test]
fn edge_a_permission_value_of_the_wrong_type_for_the_verifier_is_rejected() {
    let mut config = shipped_config();
    config["agent"]["verifier"]["permission"]["bash"] = serde_json::json!(7);
    let result = check_document(&config.to_string());
    assert!(
        result.is_err(),
        "a number is not an action opencode can apply, and it must come back as an error"
    );
    let error = result.expect_err("a number is not an action opencode can apply");
    assert!(
        error.contains("must be a string action"),
        "the error must say the value is not an action, got: {error}"
    );
}
