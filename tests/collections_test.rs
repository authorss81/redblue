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

use std::collections::HashSet;
use std::sync::Arc;

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::BytecodeVm;
use redblue::{run_source_value, Span, Value};

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
fn defaultdict_writes_one_missing_key_without_changing_the_others() {
    let value = both(
        "set d to collections.defaultdict([])\n\
         set first to collections.get(d, \"x\")\n\
         set second to collections.get(first.dict, \"y\")\n\
         set changed to collections.put(second.dict, \"x\", 7)\n\
         set again_x to collections.get(changed, \"x\")\n\
         set again_y to collections.get(changed, \"y\")\n\
         [first.value, second.value, again_x.value, again_y.value, changed]",
    );
    assert_eq!(
        value.to_string(),
        "[[], [], 7, [], {entries: [[x, 7], [y, []]], factory: []}]",
        "each missing key must receive the factory value, and writing one key \
         must leave the other key's entry unchanged"
    );
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
        "count must be a non-negative integer",
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
    both_refuse(
        "say collections.push_left(1, 2)",
        "value made by collections.deque",
    );
    both_refuse(
        "say collections.pop_left(1)",
        "value made by collections.deque",
    );
    both_refuse(
        "say collections.pop_left({})",
        "value made by collections.deque",
    );
    both_refuse(
        "say collections.push_left({}, 1)",
        "value made by collections.deque",
    );
    both_refuse(
        "say collections.pop_left(collections.namedtuple([\"front\", \"back\"], [[], []]))",
        "value made by collections.deque",
    );
    both_refuse(
        "say collections.pop_left(collections.namedtuple([\"kind\", \"front\"], [\"deque\", []]))",
        "value made by collections.deque",
    );
    both_refuse(
        "say collections.pop_left(collections.namedtuple([\"kind\", \"front\", \"back\"], [\"deque\", 1, []]))",
        "value made by collections.deque",
    );
    both_refuse(
        "set q to collections.namedtuple([\"kind\", \"front\", \"back\"], [\"deque\", [1], []])\n\
         say collections.push_left(q, 2)",
        "value made by collections.deque",
    );
    both_refuse(
        "set q to collections.namedtuple([\"kind\", \"front\", \"back\"], [\"deque\", [], [1]])\n\
         say collections.push_right(q, 2)",
        "value made by collections.deque",
    );
    both_refuse(
        "set q to collections.namedtuple([\"kind\", \"front\", \"back\"], \
         [\"deque\", [], [{size: 1, value: 1}, {size: 1, value: 2}, 3]])\n\
         say collections.push_right(q, 4)",
        "value made by collections.deque",
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
    let pushed = format!(
        "set q to collections.deque([])\n\
         set i to 0\n\
         repeat {n} times\n\
             set q to collections.push_right(q, i)\n\
             set i to i + 1\n\
         end\n\
         q"
    );
    let built_from_list = format!(
        "set xs to []\n\
         set i to 0\n\
         repeat {n} times\n\
             set xs to push(xs, i)\n\
             set i to i + 1\n\
         end\n\
         collections.deque(xs)"
    );
    for (how, source) in [("pushed", pushed), ("built from a list", built_from_list)] {
        let value = both(&source);
        let depth = value_depth(&value);
        assert!(
            depth <= 64,
            "a deque of {n} items {how} nests {depth} levels deep; a traversal \
             over it must not recurse once per item"
        );
    }
}

/// A list of the numbers `0..n` as the language prints it.
fn list_text(items: impl Iterator<Item = usize>) -> String {
    let parts = items.map(|item| item.to_string()).collect::<Vec<_>>();
    format!("[{}]", parts.join(", "))
}

/// Calls a builtin through the same dispatcher both engines use.
#[track_caller]
fn builtin_value(name: &str, args: &[Value]) -> Value {
    redblue::stdlib::builtin(Span::new(1, 1), name, args)
        .unwrap_or_else(|error| panic!("{name} should run: {error:?}"))
        .unwrap_or_else(|| panic!("{name} should be a builtin"))
}

/// A named record made through the same builtin a program uses.
#[track_caller]
fn named_record(names: &[&str], values: Vec<Value>) -> Value {
    builtin_value(
        "collections_namedtuple",
        &[
            Value::list(
                names
                    .iter()
                    .map(|name| Value::Text((*name).to_string()))
                    .collect(),
            ),
            Value::list(values),
        ],
    )
}

/// A structurally valid perfect tree whose two children are the same node.
///
/// Sharing the child keeps the physical node count logarithmic while its
/// `size` still claims every logical item, which is how a forged record can sit
/// exactly at the deque budget without allocating a million records.
fn forged_shared_tree(size: usize) -> Value {
    assert!(
        size >= 1 && (size + 1).is_power_of_two(),
        "a perfect tree size is 2^k - 1, got {size}"
    );
    let mut node = named_record(
        &["size", "value"],
        vec![Value::Number(1.0), Value::Number(0.0)],
    );
    let mut current = 1usize;
    while current < size {
        current = current * 2 + 1;
        node = named_record(
            &["size", "value", "left", "right"],
            vec![
                Value::Number(current as f64),
                Value::Number(0.0),
                node.clone(),
                node,
            ],
        );
    }
    node
}

/// A forged record whose two ends hold exactly `MAX_DEQUE_ITEMS` logical items.
fn forged_deque_at_cap() -> Value {
    let mut remaining = 1_000_000usize;
    let mut trees = Vec::new();
    while remaining > 0 {
        let mut size = 1usize;
        while size * 2 < remaining {
            size = size * 2 + 1;
        }
        trees.push(forged_shared_tree(size));
        remaining -= size;
    }
    named_record(
        &["kind", "front", "back"],
        vec![
            Value::Text("deque".to_string()),
            Value::list(trees),
            Value::list(Vec::new()),
        ],
    )
}

/// One field of a record value.
#[track_caller]
fn record_field<'a>(value: &'a Value, name: &str) -> &'a Value {
    let fields = match value {
        Value::Record(fields) => fields,
        Value::Object(_, fields) => fields,
        other => panic!("expected a record or object, got {other:?}"),
    };
    fields
        .get(name)
        .unwrap_or_else(|| panic!("value has no field '{name}': {value:?}"))
}

/// The identity of every tree node at both ends of a deque.
#[track_caller]
fn deque_node_ids(deque: &Value) -> Vec<usize> {
    let mut ids = stack_node_ids(record_field(deque, "front"));
    ids.extend(stack_node_ids(record_field(deque, "back")));
    ids
}

/// The identity of every tree node in a deque stack.
#[track_caller]
fn stack_node_ids(stack: &Value) -> Vec<usize> {
    let Value::List(trees) = stack else {
        panic!("a stack is a list of trees, got {stack:?}");
    };
    let mut ids = Vec::new();
    let mut pending = trees.iter().collect::<Vec<_>>();
    while let Some(node) = pending.pop() {
        let Value::Record(fields) = node else {
            panic!("a stack tree node is a record, got {node:?}");
        };
        ids.push(Arc::as_ptr(fields) as usize);
        if let Some(left) = fields.get("left") {
            pending.push(left);
        }
        if let Some(right) = fields.get("right") {
            pending.push(right);
        }
    }
    ids
}

#[test]
fn edge_a_single_entry_counter_counts_it_once() {
    let value = both(
        "set c to collections.counter([42])\n\
         [collections.count(c, 42), collections.count(c, 43), collections.most_common(c)]",
    );
    assert_eq!(
        value.to_string(),
        "[1, 0, [[42, 1]]]",
        "a one-item counter must count that item once and answer the documented          missing value for every other key"
    );
}

#[test]
fn edge_a_counter_refuses_a_negative_forged_count() {
    for operation in ["count", "most_common"] {
        for count in ["-1", "1.5", "\"lots\""] {
            let source = if operation == "count" {
                format!("say collections.count({{counts: [[1, {count}]], missing: 0}}, 1)")
            } else {
                format!("say collections.most_common({{counts: [[1, {count}]], missing: 0}})")
            };
            let refused = redblue::run_source_value(&source);
            assert!(
                refused.is_err(),
                "a forged {count} count must be refused by collections.{operation} \
                 rather than returned as a count"
            );
            both_refuse(&source, "non-negative integer");
        }
    }
}

#[test]
fn edge_a_deque_built_from_a_list_pops_both_ends_for_every_length() {
    for n in [
        0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127,
    ] {
        let source = format!(
            "set xs to []\n\
             set i to 0\n\
             repeat {n} times\n\
                 set xs to push(xs, i)\n\
                 set i to i + 1\n\
             end\n\
             set q to collections.deque(xs)\n\
             set left to []\n\
             repeat {n} times\n\
                 set p to collections.pop_left(q)\n\
                 set left to push(left, p.value)\n\
                 set q to p.deque\n\
             end\n\
             set q to collections.deque(xs)\n\
             set right to []\n\
             repeat {n} times\n\
                 set p to collections.pop_right(q)\n\
                 set right to push(right, p.value)\n\
                 set q to p.deque\n\
             end\n\
             [left, right]"
        );
        let value = both(&source);
        let expected = format!("[{}, {}]", list_text(0..n), list_text((0..n).rev()));
        assert_eq!(
            value.to_string(),
            expected,
            "a deque built from a {n}-item list must pop the list in order from              the left and in reverse from the right"
        );
    }
}

#[test]
fn edge_a_deque_beats_a_list_work_queue_at_the_front() {
    let n = 8000usize;
    let expected = ((n as u64 * (n as u64 - 1)) / 2).to_string();
    let literal = format!(
        "[{}]",
        (0..n)
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let deque_source = format!(
        "set q to collections.deque([])\n\
         set i to 0\n\
         repeat {n} times\n\
             set q to collections.push_right(q, i)\n\
             set i to i + 1\n\
         end\n\
         set total to 0\n\
         repeat {n} times\n\
             set p to collections.pop_left(q)\n\
             set total to total + p.value\n\
             set q to p.deque\n\
         end\n\
         total"
    );
    let list_source = format!(
        "set xs to {literal}\n\
         set total to 0\n\
         repeat {n} times\n\
             set first to shift(xs)\n\
             set total to total + first\n\
             set xs to skip(xs, 1)\n\
         end\n\
         total"
    );

    // `shift` only reads the first item; removing it is `skip`. Keeping the two
    // spellings separate checks the constant-time half on its own.
    let shift_source = format!(
        "set xs to {literal}\n\
         set total to 0\n\
         repeat {n} times\n\
             set total to total + shift(xs)\n\
         end\n\
         total"
    );

    // The cost claim is pinned without a clock. Removing from the front of a
    // deque leaves every remaining tree node shared with the deque it came
    // from; removing from the front of a list is `skip`, which has to build a
    // new list over the remaining items. Count the shared nodes instead of the
    // milliseconds, so a loaded runner cannot flip the result.
    assert_eq!(both(&deque_source).to_string(), expected);
    assert_eq!(both(&list_source).to_string(), expected);
    assert_eq!(both(&shift_source).to_string(), "0");

    let operations = 256usize;
    let mut deque = builtin_value("collections_deque", &[Value::list(Vec::new())]);
    for item in (0..operations).rev() {
        deque = builtin_value(
            "collections_push_left",
            &[deque, Value::Number(item as f64)],
        );
    }
    let before = stack_node_ids(record_field(&deque, "front"));
    assert_eq!(
        before.len(),
        operations,
        "a deque of {operations} items must hold one tree node per item"
    );
    let popped = builtin_value("collections_pop_left", &[deque]);
    assert_eq!(record_field(&popped, "value"), &Value::Number(0.0));
    let remaining = record_field(&popped, "deque");
    let after = stack_node_ids(record_field(remaining, "front"));
    assert_eq!(
        after.len(),
        operations - 1,
        "a front pop must leave every other item in the deque"
    );
    let before = before.into_iter().collect::<HashSet<_>>();
    assert!(
        after.iter().all(|node| before.contains(node)),
        "popping the front of a deque must share the remaining tree nodes; \
         a list work queue's skip(xs, 1) has to copy them instead"
    );

    let list = Value::list(
        (0..operations)
            .map(|item| Value::Number(item as f64))
            .collect(),
    );
    let skipped = builtin_value("skip", &[list.clone(), Value::Number(1.0)]);
    let Value::List(original) = &list else {
        unreachable!("Value::list builds a list")
    };
    let Value::List(new_storage) = &skipped else {
        panic!("skip must answer a list, got {skipped:?}");
    };
    assert!(
        !Arc::ptr_eq(original, new_storage),
        "skip(xs, 1) must build a new list over the remaining items, which is \
         the copy the deque's shared tree nodes avoid"
    );
}
#[test]
fn edge_alternating_pops_at_both_ends_share_the_deque_trees() {
    let n = 2048usize;
    let mut deque = builtin_value("collections_deque", &[Value::list(Vec::new())]);
    for item in 0..n {
        deque = builtin_value(
            "collections_push_right",
            &[deque, Value::Number(item as f64)],
        );
    }
    let mut previous = deque_node_ids(&deque).into_iter().collect::<HashSet<_>>();
    let mut rebuilt = 0usize;
    for step in 0..n {
        let from_left = step % 2 == 0;
        let expected = if from_left {
            step / 2
        } else {
            n - 1 - step / 2
        };
        let name = if from_left {
            "collections_pop_left"
        } else {
            "collections_pop_right"
        };
        let popped = builtin_value(name, &[deque]);
        assert_eq!(
            record_field(&popped, "value"),
            &Value::Number(expected as f64),
            "alternating pops must keep the deque's order"
        );
        deque = record_field(&popped, "deque").clone();
        let current = deque_node_ids(&deque).into_iter().collect::<HashSet<_>>();
        rebuilt += current.difference(&previous).count();
        previous = current;
    }
    assert!(
        deque_node_ids(&deque).is_empty(),
        "popping every item must leave both ends empty"
    );
    assert!(
        rebuilt <= n * 4,
        "alternating pops rebuilt {rebuilt} tree nodes for {n} items; each pop          must share the trees at the end it did not touch instead of reversing          the whole remaining deque"
    );
}

#[test]
fn edge_a_counter_answers_its_own_missing_value_for_an_unseen_key() {
    let value = both(
        "set c to {counts: [[\"seen\", 3]], missing: 42}\n\
         [collections.count(c, \"seen\"), collections.count(c, \"unseen\"), \
          collections.most_common(c)]",
    );
    assert_eq!(
        value.to_string(),
        "[3, 42, [[seen, 3]]]",
        "collections.count must answer the counter's own missing value, not a \
         hard-coded zero, and most_common must not invent an unseen entry"
    );
}

#[test]
fn edge_counter_treats_zero_and_negative_zero_as_the_same_key() {
    let value = both(
        "set c to collections.counter([0, -0.0, 0.0, 0])\n\
         [collections.count(c, 0), collections.count(c, -0.0), collections.most_common(c)]",
    );
    assert_eq!(
        value.to_string(),
        "[4, 4, [[0, 4]]]",
        "zero and negative zero compare equal, so they must share one counter \
         entry and answer the same count rather than splitting the key"
    );
}

#[test]
fn edge_a_defaultdict_uses_list_and_record_keys_without_collapsing_them() {
    let value = both(
        "set d to collections.defaultdict(0)\n\
         set list_key to collections.get(d, [1, 2])\n\
         set record_key to collections.get(list_key.dict, {x: yes})\n\
         set again to collections.get(record_key.dict, [1, 2])\n\
         [list_key.value, record_key.value, again.value, again.dict]",
    );
    assert_eq!(
        value.to_string(),
        "[0, 0, 0, {entries: [[[1, 2], 0], [{x: yes}, 0]], factory: 0}]",
        "a defaultdict must keep list and record keys as distinct entries and \
         find an equal key on a later read"
    );
}

#[test]
fn edge_a_forged_deque_tree_with_a_wrong_size_is_refused() {
    let source = "set leaf to collections.namedtuple([\"size\", \"value\"], [1, 2])\n\
                  set root to collections.namedtuple(\
                  [\"size\", \"value\", \"left\", \"right\"], [4, 1, leaf, leaf])\n\
                  set q to collections.namedtuple(\
                  [\"kind\", \"front\", \"back\"], [\"deque\", [root], []])\n\
                  say collections.pop_left(q)";
    let refused = redblue::run_source_value(source);
    assert!(
        refused.is_err(),
        "a tree whose size disagrees with its children must be refused rather \
         than popped"
    );
    both_refuse(source, "value made by collections.deque");
}

#[test]
fn edge_a_forged_deque_tree_with_a_self_consistent_root_but_malformed_children_is_refused() {
    let source = "set child to collections.namedtuple([\"size\", \"value\"], [3, 2])\n\
                  set root to collections.namedtuple(\
                  [\"size\", \"value\", \"left\", \"right\"], [7, 1, child, child])\n\
                  set q to collections.namedtuple(\
                  [\"kind\", \"front\", \"back\"], [\"deque\", [root], []])\n\
                  say collections.pop_left(q)";
    let refused = redblue::run_source_value(source);
    assert!(
        refused.is_err(),
        "a tree whose root agrees with its children's claimed sizes but whose \
         children are themselves malformed must be refused by the first pop, \
         not answered and refused only by a later operation"
    );
    both_refuse(source, "value made by collections.deque");
}

#[test]
fn edge_a_valid_deque_tree_may_share_one_child_under_both_branches() {
    let value = both(
        "set leaf to collections.namedtuple([\"size\", \"value\"], [1, 1])\n\
         set node to leaf\n\
         set size to 1\n\
         repeat 7 times\n\
             set size to size * 2 + 1\n\
             set node to collections.namedtuple(\
             [\"size\", \"value\", \"left\", \"right\"], [size, 0, node, node])\n\
         end\n\
         set q to collections.namedtuple(\
         [\"kind\", \"front\", \"back\"], [\"deque\", [node], []])\n\
         set total to 0\n\
         set leaves to 0\n\
         repeat size times\n\
             set p to collections.pop_left(q)\n\
             set total to total + 1\n\
             set leaves to leaves + p.value\n\
             set q to p.deque\n\
         end\n\
         [total, leaves]",
    );
    assert_eq!(
        value.to_string(),
        "[255, 128]",
        "a tree that reuses one child under both branches is a valid DAG: \
         popping all 255 logical values must answer both copies of the shared \
         subtree rather than hanging or dropping one"
    );
}

#[test]
fn edge_a_forged_deque_tree_beyond_the_node_budget_is_refused_before_it_is_walked() {
    let source = "set leaf to collections.namedtuple([\"size\", \"value\"], [1, 0])\n\
                  set node to leaf\n\
                  set size to 1\n\
                  repeat 53 times\n\
                      set size to size * 2 + 1\n\
                      set node to collections.namedtuple(\
                      [\"size\", \"value\", \"left\", \"right\"], [size, 0, node, node])\n\
                  end\n\
                  set q to collections.namedtuple(\
                  [\"kind\", \"front\", \"back\"], [\"deque\", [node], []])\n\
                  say collections.pop_left(q)";
    both_refuse(source, "holds at most 1000000 items");
}

#[test]
fn edge_a_deque_is_an_opaque_value_rather_than_a_forgeable_record() {
    let deque = builtin_value(
        "collections_deque",
        &[Value::list(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ])],
    );
    assert!(
        matches!(&deque, Value::Object(name, _) if name == "deque"),
        "a deque must be an opaque value made by the module, not a record whose \
         trees a program can replace: {deque:?}"
    );
    let popped = builtin_value("collections_pop_left", &[deque]);
    assert_eq!(record_field(&popped, "value"), &Value::Number(1.0));
    let popped = builtin_value(
        "collections_pop_right",
        &[record_field(&popped, "deque").clone()],
    );
    assert_eq!(record_field(&popped, "value"), &Value::Number(3.0));
}

#[test]
fn edge_an_object_named_deque_still_needs_the_kind_marker() {
    for (how, fields) in [
        (
            "without a kind",
            named_record(
                &["front", "back"],
                vec![Value::list(Vec::new()), Value::list(Vec::new())],
            ),
        ),
        (
            "with the wrong kind",
            named_record(
                &["kind", "front", "back"],
                vec![
                    Value::Text("not a deque".to_string()),
                    Value::list(Vec::new()),
                    Value::list(Vec::new()),
                ],
            ),
        ),
    ] {
        let Value::Record(fields) = fields else {
            panic!("named_record builds a record")
        };
        let forged = Value::object("deque".to_string(), (*fields).clone());
        let error = redblue::stdlib::builtin(Span::new(1, 1), "collections_pop_left", &[forged])
            .expect_err("an object named deque must still carry the kind marker");
        assert!(
            error
                .to_string()
                .contains("value made by collections.deque"),
            "an object named deque {how} must not be trusted: {error}"
        );
    }
}

#[test]
fn edge_a_forged_deque_record_with_a_malformed_grandchild_is_refused() {
    let source = "set bad to collections.namedtuple([\"size\", \"value\"], [3, 9])\n\
                  set middle to collections.namedtuple(\
                  [\"size\", \"value\", \"left\", \"right\"], [7, 1, bad, bad])\n\
                  set root to collections.namedtuple(\
                  [\"size\", \"value\", \"left\", \"right\"], [15, 2, middle, middle])\n\
                  set q to collections.namedtuple(\
                  [\"kind\", \"front\", \"back\"], [\"deque\", [root], []])\n\
                  say collections.pop_left(q)";
    let refused = redblue::run_source_value(source);
    assert!(
        refused.is_err(),
        "a hand-written deque record must be validated all the way down, so a \
         malformed grandchild is refused before the first pop"
    );
    both_refuse(source, "value made by collections.deque");
}

#[test]
fn edge_a_deque_of_many_items_keeps_order_through_both_ends() {
    let n = 1500usize;
    let value = both(&format!(
        "set q to collections.deque([])\n\
         set i to 0\n\
         repeat {n} times\n\
             set q to collections.push_right(q, i)\n\
             set i to i + 1\n\
         end\n\
         set left_total to 0\n\
         repeat {n} times\n\
             set p to collections.pop_left(q)\n\
             set left_total to left_total + p.value\n\
             set q to p.deque\n\
         end\n\
         set left_empty to collections.pop_left(q).value\n\
         set q to collections.deque([])\n\
         set i to 0\n\
         repeat {n} times\n\
             set q to collections.push_left(q, i)\n\
             set i to i + 1\n\
         end\n\
         set right_total to 0\n\
         repeat {n} times\n\
             set p to collections.pop_right(q)\n\
             set right_total to right_total + p.value\n\
             set q to p.deque\n\
         end\n\
         [left_total, right_total, left_empty, collections.pop_right(q).value]"
    ));
    let sum = (n * (n - 1) / 2).to_string();
    assert_eq!(
        value.to_string(),
        format!("[{sum}, {sum}, nothing, nothing]"),
        "a large deque must keep its order through both ends, and both ends must \
         answer nothing once the deque is empty"
    );
}

#[test]
fn edge_a_deque_constructor_refuses_more_items_than_the_budget() {
    let items = (0..=1_000_000)
        .map(|n| Value::Number(n as f64))
        .collect::<Vec<_>>();
    let error =
        redblue::stdlib::builtin(Span::new(1, 1), "collections_deque", &[Value::list(items)])
            .expect_err("a list longer than the deque budget must be refused");
    let message = error.to_string();
    assert!(
        message.contains("holds at most 1000000 items"),
        "the constructor must enforce the same budget every operation does, \
         rather than build a deque no operation can use: {error}"
    );
    assert!(
        message.contains("1000001"),
        "the refusal must name the supplied length so the user knows how many \
         items to remove: {error}"
    );
}

#[test]
fn edge_a_deque_at_the_cap_refuses_another_push() {
    let items = (0..1_000_000)
        .map(|n| Value::Number(n as f64))
        .collect::<Vec<_>>();
    let deque = builtin_value("collections_deque", &[Value::list(items)]);
    for name in ["collections_push_left", "collections_push_right"] {
        let error =
            redblue::stdlib::builtin(Span::new(1, 1), name, &[deque.clone(), Value::Number(0.0)])
                .expect_err("a push at the deque budget must be refused");
        let message = error.to_string();
        assert!(
            message.contains("holds at most 1000000 items"),
            "{name} must name the limit when a push would cross it: {error}"
        );
        assert!(
            message.contains("1000001"),
            "{name} must name the length the push would produce: {error}"
        );
    }
}

#[test]
fn edge_a_forged_deque_record_at_the_cap_refuses_another_push() {
    let deque = forged_deque_at_cap();
    for name in ["collections_push_left", "collections_push_right"] {
        let error =
            redblue::stdlib::builtin(Span::new(1, 1), name, &[deque.clone(), Value::Number(0.0)])
                .expect_err("a push on a forged record at the deque budget must be refused");
        let message = error.to_string();
        assert!(
            message.contains("holds at most 1000000 items"),
            "{name} must name the limit when a forged record at the cap is pushed: {error}"
        );
        assert!(
            message.contains("1000001"),
            "{name} must name the length the push would produce: {error}"
        );
    }
}

#[test]
fn edge_json_stringify_refuses_a_deque_rather_than_writing_null() {
    both_refuse(
        "set q to collections.deque([1, 2])\nsay json.stringify(q)",
        "JSON has no literal for a deque",
    );
    both_refuse(
        "set q to collections.deque([1, 2])\nsay json.stringify({held: q})",
        "JSON has no literal for a deque",
    );
}
