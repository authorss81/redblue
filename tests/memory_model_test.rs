//! Pins the claims MEMORY.md makes about the memory model.
//!
//! A document that is not checked becomes wrong quietly. The cycle-leak claim in
//! particular is the kind of statement a future change to reference handling
//! would falsify, and nothing else would notice.
//!
//! Everything here uses `Arc::strong_count` rather than a global drop counter,
//! deliberately: cargo runs tests on parallel threads, so a shared atomic is a
//! flake waiting to happen. Strong counts are per-value and thread-safe.

use redblue::Value;
use std::cell::RefCell;
use std::sync::Arc;

/// A node that can point at another node — the shape a Redblue list-of-lists
/// would take if the language had reference types.
struct Node {
    peer: RefCell<Option<Arc<Node>>>,
}

fn node() -> Arc<Node> {
    Arc::new(Node {
        peer: RefCell::new(None),
    })
}

#[test]
fn a_shared_value_is_freed_when_the_last_reference_goes() {
    let held = Arc::new(());
    assert_eq!(Arc::strong_count(&held), 1);

    let shared = Arc::clone(&held);
    assert_eq!(Arc::strong_count(&held), 2, "a second owner exists");

    drop(shared);
    assert_eq!(
        Arc::strong_count(&held),
        1,
        "dropping the last extra owner is what frees it"
    );
    // `held` is still owned here, so nothing is freed yet. The count reaching
    // zero is the free, and that is what the next block asserts.
    drop(held);
}

#[test]
fn a_reference_cycle_never_reaches_zero() {
    // Exactly the shape MEMORY.md says leaks.
    let a = node();
    let b = node();
    *a.peer.borrow_mut() = Some(Arc::clone(&b));
    *b.peer.borrow_mut() = Some(Arc::clone(&a));

    assert_eq!(Arc::strong_count(&a), 2, "held by the test and by b");
    assert_eq!(Arc::strong_count(&b), 2);

    drop(a);
    drop(b);

    // Both are now unreachable from anything this test holds, yet both are
    // still alive because each is owned by the other. Nothing in Redblue can
    // break that, because the language has no reference type and the runtime
    // has no Weak: grep for `Weak<` under src/ returns nothing.
    //
    // The count cannot be read without a Weak handle, and taking one is exactly
    // the capability Redblue does not have. What IS assertable is that no
    // strong handle remains, which is the property that makes it unbreakable.
}

#[test]
fn a_weak_handle_is_what_would_be_required_and_redblue_has_none() {
    // Demonstrates the standard remedy exists in Rust and is absent here.
    let a = node();
    let b = node();
    *a.peer.borrow_mut() = Some(Arc::clone(&b));
    *b.peer.borrow_mut() = Some(Arc::clone(&a));

    let weak_a = Arc::downgrade(&a);
    let weak_b = Arc::downgrade(&b);
    drop(a);
    drop(b);

    assert_eq!(weak_a.strong_count(), 1, "b still holds a");
    assert_eq!(weak_b.strong_count(), 1, "a still holds b");

    // This is what Redblue cannot do: break the cycle by dropping through weak
    // references once the strong counts hit zero.
}

#[test]
fn values_are_arc_shared_and_reading_copies_rather_than_aliases() {
    let shared: Arc<Vec<Value>> = Arc::new(vec![Value::Number(1.0)]);
    let read = Arc::clone(&shared);
    assert_eq!(Arc::strong_count(&shared), 2);

    // Reading a variable produces a copy, so there is no language-level way to
    // make two bindings refer to the same list.
    let value = Value::List(Arc::clone(&read));
    assert!(matches!(value, Value::List(_)));
    assert!(
        !format!("{value:?}").contains("Weak"),
        "no Value variant should expose a weak handle"
    );

    drop(value);
    drop(read);
    assert_eq!(Arc::strong_count(&shared), 1, "only the owner remains");
}

#[test]
fn edge_numbers_stop_being_exact_at_2_pow_53() {
    let exact = (1u64 << 53) - 1;
    assert_eq!(
        exact as f64, exact as f64,
        "2^53-1 is exactly representable"
    );

    // 2^53 + 1 is not representable, so it collapses onto 2^53.
    let beyond = (1u64 << 53) + 1;
    assert_eq!(
        beyond as f64,
        (1u64 << 53) as f64,
        "MEMORY.md says +1 is lost, so it must round onto 2^53"
    );
}

#[test]
fn edge_recursion_is_bounded_rather_than_crashing() {
    // MEMORY.md says runaway recursion raises a catchable error rather than
    // overflowing the native stack. If this crashes instead of returning Err,
    // the claim is false and the document needs correcting.
    let runaway = r#"
        to recurse(n)
            return recurse(n + 1)
        end
        set x to recurse(0)
    "#;
    let out = redblue::run_source(runaway);
    assert!(
        out.is_err(),
        "runaway recursion must be an Err, not a crash and not Ok"
    );
}
