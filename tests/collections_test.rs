//! The `collections` module: Counter, defaultdict, deque and named records.
//!
//! The finding these tests pin is the one the phase was written from: the
//! language has no counting-with-defaults value, so counting occurrences means a
//! nested scan (quadratic) or a record with an existence check per item, and a
//! record read of a missing key answers `nothing` rather than a documented
//! default. There is also no double-ended queue, and a list work queue has no
//! constant-time way to remove from the front.
//!
//! Every test runs the program on both engines and compares the values as data,
//! not only as text: a builtin reached through `stdlib::builtin` is shared, but
//! the test still asks both engines so a future change cannot answer on one and
//! refuse on the other.

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::BytecodeVm;
use redblue::{run_source_value, Value};

/// The tree-walking engine's value for `source`.
#[track_caller]
fn tree(source: &str) -> Value {
    run_source_value(source)
        .unwrap_or_else(|error| panic!("tree-walker should run:\n{source}\n{error:?}"))
}

/// The bytecode engine's value for `source`.
#[track_caller]
fn bytecode(source: &str) -> Value {
    let chunk = compile_source(source)
        .unwrap_or_else(|error| panic!("source should compile:\n{source}\n{error:?}"));
    let mut vm = BytecodeVm::new();
    vm.run(&chunk)
        .unwrap_or_else(|error| panic!("bytecode VM should run:\n{source}\n{error:?}"))
}

/// Runs `source` on both engines and asserts they answer the same value.
#[track_caller]
fn both(source: &str) -> Value {
    let walked = tree(source);
    let compiled = bytecode(source);
    assert_eq!(
        walked.to_string(),
        compiled.to_string(),
        "the two engines disagree about the text of:\n{source}"
    );
    assert_eq!(
        walked, compiled,
        "the two engines disagree about the value of:\n{source}"
    );
    walked
}

#[test]
fn counter_counts_mixed_value_types_and_reports_missing_as_zero() {
    let value = both(
        "set c to collections.counter([1, \"1\", 1, [2], {a: 1}, [2]])\n\
         set unseen to collections.count(c, \"never seen\")\n\
         set ones to collections.count(c, 1)\n\
         set texts to collections.count(c, \"1\")\n\
         set lists to collections.count(c, [2])\n\
         set records to collections.count(c, {a: 1})\n\
         [ones, texts, lists, records, unseen]",
    );
    assert_eq!(
        value.to_string(),
        "[2, 1, 2, 1, 0]",
        "the counter must count each equal value separately and answer the \
         documented missing value 0 for a key it never saw"
    );
}

/// Runs `source` on both engines and asserts both refuse it with `needle`.
#[track_caller]
fn both_refuse(source: &str, needle: &str) {
    let walked = redblue::run_source_value(source)
        .expect_err("the tree-walker should have refused the program")
        .to_string();
    assert!(
        walked.contains(needle),
        "the tree-walker's refusal does not mention '{needle}':\n{source}\n{walked}"
    );

    let chunk = compile_source(source)
        .unwrap_or_else(|error| panic!("source should compile:\n{source}\n{error:?}"));
    let mut vm = BytecodeVm::new();
    let compiled = vm
        .run(&chunk)
        .expect_err("the bytecode VM should have refused the program")
        .to_string();
    assert!(
        compiled.contains(needle),
        "the bytecode VM's refusal does not mention '{needle}':\n{source}\n{compiled}"
    );
}

#[test]
fn most_common_orders_by_count_and_keeps_first_appearance_for_ties() {
    let value = both(
        "set c to collections.counter([\"b\", \"a\", \"b\", \"c\", \"a\", \"b\"])\n\
         [collections.most_common(c), collections.count(c, \"b\"), collections.count(c, \"z\")]",
    );
    assert_eq!(
        value.to_string(),
        "[[[b, 3], [a, 2], [c, 1]], 3, 0]",
        "most_common must order by descending count and keep the order of equal \
         counts from the counter's first-appearance order"
    );
}

#[test]
fn edge_an_empty_counter_answers_empty_and_zero() {
    let value = both(
        "set c to collections.counter([])\n\
         [c, collections.most_common(c), collections.count(c, 1)]",
    );
    assert_eq!(value.to_string(), "[{counts: [], missing: 0}, [], 0]");
}

#[test]
fn counter_counts_unicode_yes_no_nothing_and_records() {
    let value = both(
        "set c to collections.counter([\"é\", \"é\", yes, yes, nothing, {a: 1}, {a: 1}])\n\
         [collections.count(c, \"é\"), collections.count(c, yes), \
          collections.count(c, nothing), collections.count(c, {a: 1})]",
    );
    assert_eq!(value.to_string(), "[2, 2, 1, 2]");
}

#[test]
fn defaultdict_get_creates_a_missing_entry_and_returns_the_factory_value() {
    let value = both(
        "set d to collections.defaultdict(0)\n\
         set first to collections.get(d, \"a\")\n\
         set again to collections.get(first.dict, \"a\")\n\
         set changed to collections.put(again.dict, \"a\", 7)\n\
         set read to collections.get(changed, \"a\")\n\
         [first.value, again.value, read.value, collections.get(changed, \"b\").value, changed]",
    );
    assert_eq!(
        value.to_string(),
        "[0, 0, 7, 0, {entries: [[a, 7]], factory: 0}]",
        "the first read must add the key with the factory value, a later read must \
         see the stored value, and put must replace it"
    );
}

#[test]
fn defaultdict_hands_each_missing_key_its_own_copy_of_the_factory_value() {
    let value = both(
        "set d to collections.defaultdict([])\n\
         set first to collections.get(d, \"x\")\n\
         set second to collections.get(first.dict, \"y\")\n\
         [first.value, second.value]",
    );
    assert_eq!(value.to_string(), "[[], []]");
}

#[test]
fn deque_pops_from_both_ends_in_order() {
    let value = both(
        "set q to collections.deque([1, 2, 3])\n\
         set first to collections.pop_left(q)\n\
         set last to collections.pop_right(first.deque)\n\
         set middle to collections.pop_left(last.deque)\n\
         set empty to collections.pop_left(middle.deque)\n\
         [first.value, last.value, middle.value, empty.value]",
    );
    assert_eq!(
        value.to_string(),
        "[1, 3, 2, nothing]",
        "a deque must remove from the end it is asked for, and an empty deque must \
         answer nothing"
    );
}

#[test]
fn deque_interleaves_pushes_and_pops_at_both_ends() {
    let value = both(
        "set q to collections.deque([])\n\
         set q to collections.push_left(q, 1)\n\
         set q to collections.push_right(q, 2)\n\
         set q to collections.push_left(q, 0)\n\
         set q to collections.push_right(q, 3)\n\
         set a to collections.pop_left(q)\n\
         set b to collections.pop_right(a.deque)\n\
         set c to collections.pop_left(b.deque)\n\
         set d to collections.pop_right(c.deque)\n\
         [a.value, b.value, c.value, d.value]",
    );
    assert_eq!(value.to_string(), "[0, 3, 1, 2]");
}

#[test]
fn edge_a_deque_reverses_its_storage_once_when_one_side_empties() {
    let value = both(
        "set q to collections.deque([])\n\
         repeat 50 times\n\
             set q to collections.push_right(q, 1)\n\
         end\n\
         set left to collections.pop_left(q)\n\
         set q to left.deque\n\
         set right to collections.pop_right(q)\n\
         [left.value, right.value]",
    );
    assert_eq!(value.to_string(), "[1, 1]");
}

#[test]
fn namedtuple_makes_a_record_with_the_names_it_was_given() {
    let value = both(
        "set point to collections.namedtuple([\"x\", \"y\"], [3, 4])\n\
         [collections.field(point, \"x\"), collections.field(point, \"y\"), point]",
    );
    assert_eq!(value.to_string(), "[3, 4, {x: 3, y: 4}]");
}

#[test]
fn edge_an_empty_namedtuple_is_an_empty_record() {
    let value = both("collections.namedtuple([], [])");
    assert_eq!(value.to_string(), "{}");
}

#[test]
fn edge_a_namedtuple_field_name_may_be_any_text() {
    let value = both(
        "set row to collections.namedtuple([\"first name\", \"ünïcode\"], [\"Ada\", 36])\n\
         [collections.field(row, \"first name\"), collections.field(row, \"ünïcode\")]",
    );
    assert_eq!(value.to_string(), "[Ada, 36]");
}

#[test]
fn edge_a_missing_named_field_is_refused_by_name_rather_than_answering_nothing() {
    both_refuse(
        "set point to collections.namedtuple([\"x\"], [1])\n\
         say collections.field(point, \"y\")",
        "no field 'y'",
    );
}

#[test]
fn edge_a_malformed_counter_is_refused() {
    both_refuse("say collections.count([], 1)", "counter is a record");
    both_refuse(
        "say collections.count({counts: []}, 1)",
        "counter is a record",
    );
    both_refuse(
        "say collections.count({missing: 0}, 1)",
        "counter is a record",
    );
    both_refuse(
        "say collections.most_common({counts: [[1, \"many\"]], missing: 0})",
        "count must be a non-negative number",
    );
}

#[test]
fn edge_a_malformed_defaultdict_is_refused() {
    both_refuse("say collections.get(0, \"a\")", "defaultdict is a record");
    both_refuse(
        "say collections.defaultdict()",
        "requires one factory value",
    );
    both_refuse(
        "say collections.put(collections.defaultdict(0), \"a\")",
        "requires a defaultdict, a key and a value",
    );
}

#[test]
fn edge_a_malformed_deque_is_refused() {
    both_refuse("say collections.deque(1)", "zero or one list");
    both_refuse("say collections.push_left(1, 2)", "deque is a record");
    both_refuse("say collections.pop_left(1)", "deque is a record");
    both_refuse("say collections.pop_left({})", "deque is a record");
    both_refuse("say collections.push_left({}, 1)", "deque is a record");
    both_refuse(
        "say collections.pop_left(collections.namedtuple([\"front\", \"back\"], [[], []]))",
        "deque is a record",
    );
    both_refuse(
        "say collections.pop_left(collections.namedtuple([\"kind\", \"front\"], [\"deque\", []]))",
        "deque is a record",
    );
    both_refuse(
        "say collections.pop_left(collections.namedtuple([\"kind\", \"front\", \"back\"], [\"deque\", 1, []]))",
        "deque is a record",
    );
}

#[test]
fn edge_namedtuple_refuses_mismatched_names_values_and_duplicates() {
    both_refuse(
        "say collections.namedtuple([\"x\", \"y\"], [1])",
        "one value per name",
    );
    both_refuse(
        "say collections.namedtuple([1], [1])",
        "field names must be text",
    );
    both_refuse(
        "say collections.namedtuple([\"x\", \"x\"], [1, 2])",
        "duplicate field 'x'",
    );
    both_refuse(
        "say collections.field([1], \"x\")",
        "requires a named record and a text field name",
    );
}

/// A deque of n items used to be n nested `{value, next}` records, so saying,
/// comparing or dropping one recursed once per item. The stack trees keep that
/// depth logarithmic, which is asserted on the value's shape rather than on a
/// wall-clock measurement.
fn value_depth(root: &Value) -> usize {
    let mut pending = vec![(root, 1usize)];
    let mut deepest = 0;
    while let Some((value, depth)) = pending.pop() {
        deepest = deepest.max(depth);
        match value {
            Value::List(items) => {
                for item in items.iter() {
                    pending.push((item, depth + 1));
                }
            }
            Value::Record(fields) | Value::Object(_, fields) => {
                for field in fields.values() {
                    pending.push((field, depth + 1));
                }
            }
            _ => {}
        }
    }
    deepest
}

#[test]
fn edge_a_large_deque_keeps_its_traversal_depth_bounded() {
    let n = 4000;
    let source = format!(
        "set q to collections.deque([])\n\
         set i to 0\n\
         repeat {n} times\n\
             set q to collections.push_right(q, i)\n\
             set i to i + 1\n\
         end\n\
         q"
    );
    let value = both(&source);
    let depth = value_depth(&value);
    assert!(
        depth <= 64,
        "a deque of {n} items nests {depth} levels deep; a traversal over it \
         must not recurse once per item"
    );
}
