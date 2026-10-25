//! LIBRARIES.md is the standing answer to a question every stdlib phase would
//! otherwise re-open: should this be written in Redblue, wrapped around a Rust
//! crate, or not built at all?
//!
//! The policy is only useful if it is (a) actually written down, (b) reachable
//! from the two files an agent reads first, and (c) complete over the ranked
//! gaps in SURVEY.md. An undecided gap sends the next phase back to arguing from
//! scratch, which is the finding this phase exists to close. So the document is
//! checked here instead of trusted.

use std::path::PathBuf;

const POLICY: &str = "LIBRARIES.md";
const README: &str = "README.md";
const AGENTS: &str = "AGENTS.md";

/// The three decisions the policy is allowed to make.
const SECTIONS: [&str; 3] = [
    "## 1. Written in Redblue",
    "## 2. Bindings to Rust crates",
    "## 3. Not built at all",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(name: &str) -> String {
    let path = repo_root().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{name} must exist and be readable at {}: {error}. LIBRARIES.md is \
             the whole deliverable of this phase; a policy nobody wrote down is \
             the finding left unfixed.",
            path.display()
        )
    })
}

/// The body of one `## ` section, up to the next `## ` heading.
fn section<'a>(doc: &'a str, heading: &str) -> Option<&'a str> {
    let start = doc.find(heading)? + heading.len();
    let rest = &doc[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    Some(&rest[..end])
}

fn sections(doc: &str) -> Vec<&str> {
    SECTIONS
        .iter()
        .map(|heading| {
            section(doc, heading).unwrap_or_else(|| {
                panic!(
                    "LIBRARIES.md has no `{heading}` section. The policy has to \
                     separate the three answers it gives: written in Redblue, a \
                     binding, or not built at all."
                )
            })
        })
        .collect()
}

/// Every top-ten gap in SURVEY.md carries an explicit `SURVEY.md rank N` marker
/// in exactly one of the three sections. The marker is the decision; a gap with
/// no marker is a gap the next phase has to re-litigate.
#[test]
fn edge_every_ranked_gap_is_decided_in_exactly_one_section() {
    let doc = read(POLICY);
    let bodies = sections(&doc);

    for rank in 1..=10 {
        let marker = format!("rank {rank} |");
        let decided: Vec<usize> = bodies
            .iter()
            .enumerate()
            .filter(|(_, body)| body.contains(&marker))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            decided.len(),
            1,
            "{marker} appears in {} of the three sections ({decided:?}). Every \
             gap ranked in SURVEY.md has to fall on exactly one side of the \
             line, or the policy is silent about work a phase will be asked to \
             do.",
            decided.len()
        );
    }
}

/// Each section is more than a heading: it says why its entries belong there.
#[test]
fn each_section_states_a_reason_not_just_a_list() {
    let doc = read(POLICY);
    for (heading, body) in SECTIONS.iter().zip(sections(&doc)) {
        assert!(
            body.len() > 200,
            "`{heading}` is only {} bytes. A list of names with no reason is \
             taste; the policy has to say why each class belongs on that side.",
            body.len()
        );
        assert!(
            body.contains("because") || body.contains("Why"),
            "`{heading}` never gives a reason (`because` / `Why`). The phase \
             asks for entries justified by their reason, not by taste."
        );
    }
}

/// The binding section names a concrete crate for each numeric or native
/// capability, and the crates it says are already dependencies are in
/// `Cargo.toml`.
#[test]
fn binding_entries_name_specific_rust_crates() {
    let doc = read(POLICY);
    let body = section(&doc, SECTIONS[1]).expect("the binding section exists");

    let required = [
        "regex",
        "chrono",
        "walkdir",
        "glob",
        "base64",
        "hex",
        "percent-encoding",
        "sha2",
        "tch",
        "ndarray",
        "nalgebra",
        "statrs",
        "ring",
        "aes-gcm",
        "ed25519-dalek",
    ];
    for crate_name in required {
        assert!(
            body.contains(crate_name),
            "the binding section never names `{crate_name}`. A binding entry \
             without a crate is a plan that cannot be started."
        );
    }

    let manifest = read("Cargo.toml");
    for already_a_dependency in ["regex", "chrono", "reqwest"] {
        assert!(
            manifest.contains(already_a_dependency),
            "LIBRARIES.md presents `{already_a_dependency}` as the numpy-pattern \
             precedent, and Cargo.toml no longer depends on it. The precedent \
             has to be real."
        );
    }
}

/// The policy is honest about speed: it promises readability, says interpreted
/// Redblue is slower than Python, and names the native backend as the only
/// thing that would change that.
#[test]
fn the_speed_answer_is_stated_plainly_and_not_hedged() {
    let doc = read(POLICY).to_lowercase();

    for claim in [
        "slower than python",
        "readability",
        "native backend",
        "only thing",
        "pytorch",
        "numpy",
        "not achievable",
    ] {
        assert!(
            doc.contains(claim),
            "LIBRARIES.md never says `{claim}`. The policy has to state plainly \
             that a PyTorch-equivalent written in Redblue is not achievable, \
             that interpreted Redblue is slower than Python, and that a native \
             backend is the only thing that changes the speed answer."
        );
    }
}

/// An agent reaches the policy from the two files it already reads: the project
/// README and the agent contract.
#[test]
fn edge_policy_is_linked_from_readme_and_agents() {
    for name in [README, AGENTS] {
        let text = read(name);
        assert!(
            text.contains("(LIBRARIES.md)") || text.contains("[LIBRARIES.md]"),
            "{name} does not link LIBRARIES.md. A policy the next implementer \
             never sees is a policy that gets re-argued."
        );
    }
}

/// The premise of the binding section, re-verified on this tree: a program that
/// reaches for a binding-required library is refused today. If one of these
/// calls started working, the entry would describe a gap that had closed.
#[test]
fn binding_required_libraries_are_refused_today() {
    let analyzer_refusal = refusal_of(
        "say base64.decode(\"aGk=\")\n",
        "a program called `base64.decode`, which LIBRARIES.md lists as a \
         required binding. If it now runs, the policy's ranking has to be \
         re-measured.",
    );
    assert!(
        analyzer_refusal.contains("base64"),
        "`base64.decode` was refused as `{analyzer_refusal}`, which does not name \
         the library a reader would go looking for."
    );

    let runtime_refusal = refusal_of(
        "set born to time.unix(\"1990-03-04 00:00:00\")\nsay time.add_days(born, 365)\n",
        "`time.add_days` ran. LIBRARIES.md presents date arithmetic as a \
         binding-required gap; if the gap closed, the entry is stale.",
    );
    assert!(
        runtime_refusal.contains("add_days") && runtime_refusal.contains("time"),
        "`time.add_days` was refused as `{runtime_refusal}`, which does not name \
         both the module and the member."
    );
}

/// Run a program and return the first stage's error as text, panicking if the
/// whole pipeline accepts it.
fn refusal_of(source: &str, why: &str) -> String {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("the probe program lexes");
    let ast = redblue::parser::parse(tokens).expect("the probe program parses");
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return format!("{error}");
    }
    let (_vm, result) = redblue::run_isolated(&ast);
    assert!(
        result.is_err(),
        "{why} It answered `{:?}`.",
        result.as_ref().ok()
    );
    match result {
        Ok(value) => panic!("{why} It answered `{value:?}`."),
        Err(error) => format!("{error}"),
    }
}
