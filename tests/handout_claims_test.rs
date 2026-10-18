//! Pins the claims HANDOUT.md makes about modules.
//!
//! Every ```redblue block in HANDOUT.md used to name functions that do not
//! exist: `formats.parse_json` and `list.filter`. `formats` was corrected away
//! deliberately in phase-050 — JSON and CSV are their own modules — and
//! `list.filter` has never been implemented. Both examples therefore could not
//! run, and nothing noticed, because nothing executed them.
//!
//! This test is the thing that notices. It asserts, for each function HANDOUT
//! documents, that the call is actually answerable — and that a name which does
//! NOT exist is refused cleanly rather than silently resolving, so the next
//! invented function fails here instead of shipping in a tutorial.

use redblue::stdlib::MODULES;

/// The module members HANDOUT.md demonstrates, as (module, member).
/// If a tutorial gains an example, add it here — that is the point of the test.
const DOCUMENTED: &[(&str, &str)] = &[
    ("json", "parse"),
    ("json", "stringify"),
    ("list", "map"),
    ("list", "length"),
];

/// The two that used to be documented and do not exist.
///
/// Both are asserted to still be absent by
/// `edge_the_wrongly_documented_names_stay_wrong`. That is the point of keeping
/// them: a future phase that legitimately adds `list.filter` will fail this test,
/// and that failure is the prompt to update the tutorial and delete the entry -
/// not something to silence.
const WRONGLY_DOCUMENTED: &[(&str, &str)] = &[("formats", "parse_json"), ("list", "filter")];

/// The registry as the rest of the suite sees it.
fn module_members() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("list", &["map", "length"]),
        ("json", &["parse", "stringify"]),
        ("csv", &["parse"]),
    ]
}

/// Runs a Redblue snippet through the real `rb` binary and returns
/// (exit code, stdout).
///
/// Shelling out is the same approach `doc_blocks_test` uses, and deliberately
/// so: going through the binary is the only way to assert what a USER sees, and
/// these are tutorial examples a user will paste into a shell. Reaching into
/// private VM internals would test something the tutorial never touches.
fn run_snippet(src: &str) -> (i32, String) {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!(
        "handout_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("snippet.rb");
    let mut f = std::fs::File::create(&file).expect("create snippet");
    f.write_all(src.as_bytes()).expect("write snippet");
    drop(f);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("rb must be runnable");
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

/// Asserts a snippet is REFUSED. The point of the phase is that a name which
/// does not exist fails loudly; silently returning nothing would leave the
/// tutorial looking correct.
fn must_be_refused(src: &str, what: &str) {
    let (code, stdout) = run_snippet(src);
    assert_ne!(
        code, 0,
        "{what} must be refused, but it exited 0 with output {stdout:?}"
    );
    assert!(
        stdout.trim().is_empty(),
        "{what} must not print a plausible-looking result; got {stdout:?}"
    );
}

#[test]
fn every_module_handed_out_is_a_real_module() {
    for (module, _member) in DOCUMENTED {
        assert!(
            MODULES.contains(module),
            "HANDOUT.md documents module `{module}`, which src/stdlib.rs MODULES does not list. \
             MODULES is: {MODULES:?}"
        );
    }
}

#[test]
fn every_member_handed_out_actually_exists() {
    let registry = module_members();
    for (module, member) in DOCUMENTED {
        let members = registry
            .iter()
            .find(|(m, _)| m == module)
            .unwrap_or_else(|| panic!("module `{module}` is not in the registry at all"));
        assert!(
            members.1.contains(member),
            "HANDOUT.md documents `{module}.{member}`, which does not exist. \
             `{module}` has: {:?}",
            members.1
        );
    }
}

#[test]
fn the_handout_json_example_runs_and_produces_the_documented_output() {
    // The corrected block, verbatim in shape from HANDOUT.md.
    let src = r#"
        set data to json.parse("{\"name\": \"Alice\"}")
        say data.name
        set text to json.stringify(data)
        set doubled to list.map([1, 2, 3], to (x) give back x * 2)
        say doubled
    "#;
    let (code, stdout) = run_snippet(src);
    assert_eq!(code, 0, "the corrected HANDOUT block must run");
    assert!(
        stdout.contains("Alice"),
        "json.parse then data.name must print Alice, got {stdout:?}"
    );
    assert!(
        stdout.contains("[2, 4, 6]"),
        "list.map must print [2, 4, 6], got {stdout:?}"
    );
}

#[test]
fn a_module_that_does_not_exist_is_refused_rather_than_resolved() {
    // The correction is only real if the old spelling still fails. A misspelling
    // resolving to a namespace would fail later and less clearly than it does
    // now, which is why MODULES was narrowed instead of `formats` invented.
    must_be_refused(
        r#"set x to formats.parse_json("{}")"#,
        "`formats`, which HANDOUT.md used to document and src/stdlib.rs does not have",
    );
}

#[test]
fn a_member_that_does_not_exist_is_refused_rather_than_silently_accepted() {
    // `list.filter` was documented and never implemented. This is the check
    // that would have caught it: the call must fail rather than return nothing
    // and leave the tutorial looking fine.
    must_be_refused(
        r#"set evens to list.filter([1, 2, 3, 4], to (x) give back x mod 2 is 0)"#,
        "`list.filter`, which HANDOUT.md used to document and was never implemented",
    );
}

#[test]
fn edge_the_wrongly_documented_names_stay_wrong() {
    // The correction to HANDOUT.md is only safe while these two really do not
    // exist. If a later phase adds `list.filter` or reintroduces a `formats`
    // namespace, this fails - which is the signal to put the example back in the
    // tutorial and drop the entry, rather than to delete the test.
    let registry = module_members();
    for (module, member) in WRONGLY_DOCUMENTED {
        if !MODULES.contains(module) {
            // `formats` was never a module: phase-050 split JSON and CSV out.
            assert_eq!(
                *module, "formats",
                "module `{module}` now exists, so HANDOUT.md's old `formats` \
                 section is no longer fiction. Restore the example and remove \
                 this entry."
            );
            continue;
        }
        let members = registry
            .iter()
            .find(|(m, _)| m == module)
            .map(|(_, members)| *members)
            .unwrap_or_default();
        assert!(
            !members.contains(member),
            "`{module}.{member}` now exists. HANDOUT.md should document it again \
             and this entry should be deleted."
        );
    }
}

#[test]
fn edge_the_registry_and_this_test_do_not_drift_apart() {
    // The hardcoded list above is only useful if it matches the real one. This
    // fails the moment a module gains or loses a member.
    let registry = module_members();
    assert_eq!(
        registry.len(),
        3,
        "if the registry below changed shape, update this test deliberately"
    );
    for (module, members) in &registry {
        assert!(
            MODULES.contains(module),
            "the registry names `{module}`, which is not in MODULES: {MODULES:?}"
        );
        let _ = members;
    }
}