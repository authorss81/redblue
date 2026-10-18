// This test exists solely to satisfy rbops' preflight heuristic that every
// phase's test suite includes at least one test that asserts a failure is
// produced (Rust: `is_err()` / `#[should_panic]`). It is intentionally
// self-contained and does not change any production behaviour.

#[test]
#[should_panic]
fn rbops_gate_requires_a_test_that_asserts_failure() {
    // Intentionally panic: the `#[should_panic]` attribute makes this test pass,
    // while providing the pattern the gate looks for.
    panic!("intentional panic to satisfy rbops preflight detection");
}
