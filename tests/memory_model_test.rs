//! Tests that pin the memory model documented in [`docs/MEMORY.md`].
//!
//! The phase that added this file implemented nothing: the model is what the
//! code already does. These tests therefore exist so the *documentation*
//! cannot quietly become untrue. Each one states a claim `MEMORY.md` makes and
//! checks it against the code, and the citation test at the bottom checks that
//! the line numbers `MEMORY.md` points at still say what it says they say.
//!
//! Two of them measure rather than inspect. The sharpest claim in `MEMORY.md` is
//! about memory that is never handed back, and the only way a test can fail when
//! that stops being true is to watch the bytes go up and not come down. So this
//! file installs a counting allocator and reports *live bytes* for the cycle
//! case, and reports the same number for a program that is supposed to hand
//! everything back.
//!
//! [`docs/MEMORY.md`]: ../docs/MEMORY.md

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use redblue::Value;

/// Bytes handed out by the allocator and not yet handed back, crate-wide.
///
/// A leak is memory this number refuses to give up, so it is the one observation
/// that says "never reclaimed" rather than "still reachable". Counted in bytes
/// rather than in allocations because the claim `MEMORY.md` makes is about the
/// payload of a list, and a count of allocation *headers* would miss it.
static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);

/// The system allocator, with a running total of what it has not given back.
struct Counting;

/// Every method is a `System` call plus one atomic add, so installing this
/// changes what the suite measures and nothing else: the layouts are passed
/// through untouched, and `dealloc` is handed the layout its `alloc` was, which
/// is the contract that makes the two halves of the counter agree.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        unsafe { System.dealloc(block, layout) };
        LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let block = unsafe { System.alloc_zeroed(layout) };
        if !block.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        block
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(block, layout, new_size) };
        if !moved.is_null() {
            LIVE_BYTES.fetch_add(new_size as isize, Ordering::Relaxed);
            LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The bytes currently held, as a process-wide count.
///
/// Read under [`exclusive`], and only ever as a difference: another test that
/// ran first may have leaked on purpose, and its bytes are in the count for
/// good, so a delta against a baseline taken at the same moment is the only
/// figure that means anything.
fn live_bytes() -> isize {
    LIVE_BYTES.load(Ordering::Relaxed)
}

/// The lock every measurement and every environment variable in this file is
/// taken under.
///
/// Both are process-wide, so two tests doing either at once would read each
/// other's numbers: the call-depth override is read by every VM in the process,
/// and a cycle one test leaks stays on the byte count for every test after it.
/// Poisoning is ignored rather than propagated — a test that panicked has
/// already reported, and the bytes it counted are still worth reading.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The source file a citation in `MEMORY.md` names, read as a vector of lines.
fn source_lines(relative: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} should be readable: {}", path.display(), error));
    text.lines().map(str::to_owned).collect()
}

/// The one-based line `n` of `relative`, or a panic naming the file.
fn source_line(relative: &str, n: usize) -> String {
    let lines = source_lines(relative);
    assert!(
        n >= 1 && n <= lines.len(),
        "{} has {} lines, so line {} cannot be cited",
        relative,
        lines.len(),
        n
    );
    lines[n - 1].clone()
}

fn readme_text() -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("README.md should be readable")
}

fn memory_text() -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/MEMORY.md"))
        .expect("docs/MEMORY.md should be readable")
}

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// Runs `source` on a thread sized for the call depth, under [`exclusive`], and
/// returns the value of its last statement.
///
/// The lock is taken here rather than at each call site because the limit a
/// program runs under is read out of the process environment, so a test that
/// overrides it and a test that merely runs a program are measuring each other
/// unless one of them waits.
#[track_caller]
fn eval(source: &str) -> Value {
    let _guard = exclusive();
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.unwrap_or_else(|error| panic!("source should have run, failed with {:?}", error))
}

/// Runs an already-parsed `program` and returns the error it produced.
///
/// The caller holds [`exclusive`], because a program reads the call-depth limit
/// out of the process environment and two tests disagreeing about it is a race
/// with a test result rather than with another thread.
#[track_caller]
fn run_err(program: &redblue::parser::Program) -> redblue::Error {
    let (_vm, result) = redblue::run_isolated(program);
    result.expect_err("the program should have failed")
}

/// `MEMORY.md`: "values are immutable once constructed and shared".
///
/// A list, a record and an object all hand out the *same* allocation, so
/// cloning a `Value` raises the handle count rather than copying the payload.
/// `MEMORY.md` is claiming the representation is `Arc`, and this is what that
/// claim looks like from outside.
#[test]
fn list_record_and_object_storage_is_shared_not_copied() {
    let list = Value::list(vec![Value::Number(1.0), Value::Number(2.0)]);
    let Value::List(list_storage) = list.clone() else {
        panic!("Value::list should build a List, got {:?}", list)
    };
    assert_eq!(
        Arc::strong_count(&list_storage),
        2,
        "a cloned list should share one allocation, not make a second"
    );
    assert_eq!(
        list_storage.as_ref(),
        &[Value::Number(1.0), Value::Number(2.0)],
        "the shared payload should be the list that was built"
    );

    let record = Value::record(Default::default());
    let Value::Record(record_storage) = record.clone() else {
        panic!("Value::record should build a Record, got {:?}", record)
    };
    assert_eq!(
        Arc::strong_count(&record_storage),
        2,
        "a cloned record should share one allocation"
    );

    let object = Value::object("Node".to_string(), Default::default());
    let Value::Object(name, object_storage) = object.clone() else {
        panic!("Value::object should build an Object, got {:?}", object)
    };
    assert_eq!(name, "Node", "the type name is beside the shared fields");
    assert_eq!(
        Arc::strong_count(&object_storage),
        2,
        "a cloned object should share one allocation"
    );
}

/// `MEMORY.md`: "reading a variable copies rather than aliases".
///
/// This is the observable consequence, and it is the rule a program is written
/// against: a list bound to one name and grown through another name is not a
/// mutation of the first, so the two never become aliases.
#[test]
fn reading_a_variable_copies_rather_than_aliases() {
    let source = concat!(
        "set first to [1, 2]\n",
        "set second to first\n",
        "set second to push(second, 3)\n",
        "length(first) - length(second)\n",
    );

    assert_eq!(
        eval(source),
        Value::Number(-1.0),
        "growing `second` must not be visible through `first`: they are two \
         bindings of one shared allocation, and the write copies before it writes"
    );
}

/// `MEMORY.md`: the same rule, checked on the storage itself.
///
/// Two names hold two handles to one allocation, so the second name's append
/// has to copy before it writes — that is `Arc::make_mut` doing its job, and it
/// is why an append cannot be seen through the name the list was made with.
#[test]
fn edge_writing_through_one_name_copies_so_the_other_name_cannot_see_it() {
    let mut list = Value::list(vec![Value::Number(1.0)]);
    let Value::List(storage) = list.clone() else {
        panic!("Value::list should build a List")
    };
    assert_eq!(
        Arc::strong_count(&storage),
        2,
        "both names should start on the same allocation"
    );

    let before = storage.as_ref().len();
    assert!(
        list.append(Value::Number(2.0)),
        "appending to a list should report the growth"
    );
    let Value::List(after_storage) = list.clone() else {
        panic!("the appended value is still a List")
    };

    assert_eq!(
        storage.as_ref().len(),
        before,
        "the shared allocation the other name holds must not have grown"
    );
    assert_eq!(
        after_storage.as_ref().len(),
        before + 1,
        "the appended-to name should see its own element"
    );
    assert_eq!(
        before + 1,
        2,
        "growing a shared list copies, so exactly one element was added"
    );
}

/// The program whose values the cycle below is built out of.
///
/// Both nodes are records a Redblue program made, and both share one list it
/// grew element by element, so the thing the cycle fails to release is a real
/// `Value` of a real size rather than a stand-in type with a number in it.
const CYCLE_SOURCE: &str = concat!(
    "set payload to []\n",
    "set i to 1\n",
    "repeat 400000 times\n",
    "    append(\"payload\", i)\n",
    "    set i to i + 1\n",
    "end\n",
    "set head to { name: \"head\", payload: payload }\n",
    "set tail to { name: \"tail\", payload: payload }\n",
    "[head, tail]\n",
);

/// The elements the payload list `CYCLE_SOURCE` grows.
const PAYLOAD_ELEMENTS: usize = 400_000;

/// The bytes a cycle holding that payload must still be holding, and the bytes a
/// finished program must not be.
///
/// The same number read in opposite directions, because the two are the same
/// measurement: a `Vec<Value>` of `PAYLOAD_ELEMENTS` elements is several times
/// this whatever `Value` turns out to weigh, and the gap between the two bounds
/// is far wider than anything else in this binary allocates at once — every
/// other test here either runs under [`exclusive`] or allocates kilobytes.
const PAYLOAD_FLOOR: isize = 4 * 1024 * 1024;
const PAYLOAD_CEILING: isize = 1024 * 1024;

/// The two records `CYCLE_SOURCE` ends with, with the program's own handles to
/// them dropped.
///
/// This is the point where the bytes the program built exist and belong to
/// nobody the caller is holding — only the two clones returned here. The caller
/// holds [`exclusive`], so the byte count can be sampled either side of this
/// and the difference is the program's and nothing else's.
#[track_caller]
fn program_records() -> Vec<Value> {
    let (_vm, result) = redblue::run_isolated(&parse(CYCLE_SOURCE));
    let result =
        result.unwrap_or_else(|error| panic!("the program should have run, got {error:?}"));
    let Value::List(records) = result else {
        panic!("{} should end with a list of two records", "CYCLE_SOURCE")
    };
    let records = records.as_ref().clone();
    assert_eq!(
        records.len(),
        2,
        "the program should end with the head and the tail, one name each"
    );
    records
}

/// A record joined to its successor by a back-reference, which is what an `Arc`
/// graph becomes cyclic when one node's storage holds a handle to another that
/// holds a handle back.
///
/// `value` is a `Value` a Redblue program built, held in the very
/// `std::sync::Arc` that `Shared` is an alias for
/// (`pub type Shared<T> = std::sync::Arc<T>;`, `src/value.rs:276`), and `next`
/// is the one capability Redblue's own representation lacks: a slot that can be
/// filled *after* the handle to the thing holding it exists. That is precisely
/// what `Shared<Fields>` cannot do, and precisely why no Redblue program can
/// close this loop — a container is filled before the handle to it is made.
/// Supplying it here is what turns the leak from a sentence in a document into
/// bytes this file can count.
struct Node {
    value: Mutex<Option<Value>>,
    next: Mutex<Option<Arc<Node>>>,
}

/// Joins `records` into a cycle and drops every handle that is not part of it,
/// leaving the only observer a cycle collector would have to consult: a `Weak`
/// back-reference, which counts for nothing and so cannot keep anything alive
/// on its own.
fn build_cycle(records: Vec<Value>) -> Weak<Node> {
    let head = Arc::new(Node {
        value: Mutex::new(Some(records[0].clone())),
        next: Mutex::new(None),
    });
    let tail = Arc::new(Node {
        value: Mutex::new(Some(records[1].clone())),
        next: Mutex::new(None),
    });
    *head.next.lock().expect("the head is not poisoned") = Some(tail.clone());
    *tail.next.lock().expect("the tail is not poisoned") = Some(head.clone());

    let observer = Arc::downgrade(&head);
    drop(records);
    drop(head);
    drop(tail);
    observer
}

/// `MEMORY.md`: "a reference cycle is never reclaimed at all".
///
/// Asserted in bytes rather than in shape, because "never reclaimed" is a claim
/// about memory and a shape assertion cannot fail when it stops being true. The
/// cycle is built from records a Redblue program made, every handle that is not
/// part of the cycle is dropped, and the allocator is then asked how much is
/// still held: the whole payload of the program's list, with nothing outside the
/// cycle naming it. Dropping the last observer afterwards must not move that
/// number at all, which is the part a `Weak`-based collector would have to fix
/// and the part nothing in `src/` does.
#[test]
fn edge_a_reference_cycle_is_never_reclaimed() {
    let _guard = exclusive();
    let baseline = live_bytes();

    let observer = build_cycle(program_records());
    let retained = live_bytes() - baseline;
    assert!(
        retained >= PAYLOAD_FLOOR,
        "a cycle holding a {PAYLOAD_ELEMENTS}-element list should still be holding \
         at least {PAYLOAD_FLOOR} bytes with nothing outside it, and is holding {}",
        retained
    );

    let head = observer
        .upgrade()
        .expect("an `Arc` cycle with no `Weak` back-reference and no collector is never freed");
    assert_eq!(
        Arc::strong_count(&head),
        2,
        "after both external handles are dropped the head is held only by the \
         cycle's own two edges, and nothing else"
    );

    let head_record = head.value.lock().expect("the head is not poisoned");
    let Value::Record(fields) = head_record
        .as_ref()
        .expect("the head should still hold the record the program made")
    else {
        panic!(
            "the program's `head` record should still be a record, got {:?}",
            head_record
        )
    };
    assert_eq!(
        fields.get("name"),
        Some(&Value::Text("head".to_string())),
        "the record inside the cycle is the one the program built, not a stand-in"
    );
    let Value::List(items) = fields
        .get("payload")
        .expect("the record should hold its payload")
    else {
        panic!("the payload should still be the list the program grew")
    };
    assert_eq!(
        items.len(),
        PAYLOAD_ELEMENTS,
        "the whole payload survived the cycle, which is the whole point: it was \
         never freed"
    );
    assert_eq!(
        Arc::strong_count(items),
        2,
        "the payload list is held by the two records of the cycle and by nothing \
         outside it"
    );
    assert_eq!(
        Arc::strong_count(fields),
        1,
        "the head's own record storage is held by the head node alone, so what is \
         leaking is Redblue's record storage and not a test type"
    );

    let tail = head
        .next
        .lock()
        .expect("the head is not poisoned")
        .clone()
        .expect("the head should still point at the tail");
    let back = tail
        .next
        .lock()
        .expect("the tail is not poisoned")
        .clone()
        .expect("the cycle's back-edge should still be set");
    assert!(
        Arc::ptr_eq(&back, &head),
        "the cycle should still close: head -> tail -> head, with nothing \
         outside it holding either node"
    );

    let before_the_last_observer = live_bytes();
    drop(head_record);
    drop(back);
    drop(tail);
    drop(head);

    let released = before_the_last_observer - live_bytes();
    assert!(
        released < PAYLOAD_CEILING,
        "the last handle outside the cycle went away and released {released} \
         bytes. A reference cycle is never reclaimed, so letting go of every \
         external handle must give nothing back at all."
    );
    assert!(
        live_bytes() - baseline >= PAYLOAD_FLOOR,
        "with every handle outside the cycle dropped, the payload of the list \
         the program grew is still allocated — {} bytes above the baseline",
        live_bytes() - baseline
    );
}

/// `MEMORY.md`: "**today** no Redblue program can build a cycle".
///
/// The other half of the leak test, and the half that says where the boundary
/// is. The same program, the same allocator, and the opposite result: it runs,
/// the VM and its result are dropped, and every byte it allocated is handed
/// back. A cycle reachable from a program would be the one thing that could not
/// be true — the program in `edge_a_reference_cycle_is_never_reclaimed` keeps
/// exactly what this one gives up.
#[test]
fn edge_a_redblue_program_hands_back_everything_it_allocated() {
    let _guard = exclusive();
    let baseline = live_bytes();

    let program = parse(CYCLE_SOURCE);
    {
        let (vm, result) = redblue::run_isolated(&program);
        assert!(
            result.is_ok(),
            "the program should run, got {:?}",
            result.err()
        );
        assert!(
            live_bytes() - baseline >= PAYLOAD_FLOOR,
            "the program should really have allocated the payload the cycle is \
             measured against, and is holding {} bytes",
            live_bytes() - baseline
        );
        drop(result);
        drop(vm);
    }

    let retained = live_bytes() - baseline;
    assert!(
        retained < PAYLOAD_CEILING,
        "a program that has finished must hand back everything it allocated, \
         and {retained} bytes were still held"
    );
}

/// `MEMORY.md`: "a write builds a new value rather than changing the old one" —
/// and, at the type level, why that is also why a cycle cannot be closed.
///
/// `src/interpreter.rs:1625` hands `Arc::make_mut` a *clone* of the storage and
/// `src/interpreter.rs:1626` hands it a value that is itself a second live
/// handle to that storage, so the storage is shared twice over and `make_mut`
/// copies. This is that write, on real `Shared<Fields>` storage, asserting on the
/// two allocations rather than on a message: the field ends up holding the record
/// *as it was before the write*, which is the whole reason
/// `a_redblue_program_cannot_build_a_reference_cycle` answers `nothing`.
#[test]
fn edge_writing_a_record_into_its_own_field_copies_instead_of_closing_a_cycle() {
    let record = Value::record(Default::default());
    let Value::Record(original) = record.clone() else {
        panic!("Value::record should build a Record, got {:?}", record)
    };
    let Value::Record(mut writable) = record.clone() else {
        panic!("Value::record should build a Record, got {:?}", record)
    };
    assert_eq!(
        Arc::strong_count(&original),
        3,
        "the binding and both reads hold one allocation before the write"
    );

    let stored = Value::Record(writable.clone());
    Arc::make_mut(&mut writable).insert("self".to_string(), stored);

    assert!(
        !Arc::ptr_eq(&writable, &original),
        "`make_mut` must copy rather than write through, because the value being \
         stored is a live handle to the storage being written"
    );

    let Value::Record(inner) = writable
        .get("self")
        .expect("the write should have put the record in its own field")
        .clone()
    else {
        panic!("the stored value should be a record")
    };
    assert!(
        Arc::ptr_eq(&inner, &original),
        "the field holds the record as it was before the write, which is why \
         `node.self.self` is `nothing` rather than the record itself"
    );
    assert!(
        !Arc::ptr_eq(&inner, &writable),
        "the storage that now holds the field is not the storage the field \
         points at, so nothing here is a cycle"
    );
}

/// `MEMORY.md`: "a Redblue container is filled before the handle to it exists".
///
/// The reason no program can close the cycle above, checked in the
/// representation rather than in any code path: nothing in the value module can
/// be filled in after its handle exists, and nothing in the crate holds a
/// back-reference it could be filled with. Either one landing is what turns the
/// latent limitation into a reachable one, so both are read off the source.
#[test]
fn a_redblue_value_has_no_interior_mutability_and_no_back_reference() {
    let value_rs =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/value.rs"))
            .expect("src/value.rs should be readable");

    for forbidden in [
        "RefCell",
        "Cell<",
        "Mutex",
        "UnsafeCell",
        "Rc<",
        "Rc::",
        "Weak",
        "::downgrade",
        "::upgrade",
        "Gc",
        "from_raw",
        "into_raw",
    ] {
        assert!(
            !value_rs.contains(forbidden),
            "src/value.rs mentions `{}`. A value a program can reach into after \
             construction is what closes a cycle, and there is none: `Shared` is \
             `Arc`, and `Arc` hands out a `&mut` only to the last handle.",
            forbidden
        );
    }
}

/// `MEMORY.md`: "no Redblue program can build a cycle today".
///
/// The nearest a program gets is storing a record through its own field. The
/// write goes through `Arc::make_mut` (src/interpreter.rs:1626), and because
/// the value being stored is itself a live handle the storage is copied before
/// it is written to — so the copy `node.self` holds is the record *as it was
/// before* the write, not the record after it. A cycle would need the storage
/// to contain a handle to itself, and here it does not.
#[test]
fn a_redblue_program_cannot_build_a_reference_cycle() {
    let source = concat!(
        "set node to { name: \"node\", self: nothing }\n",
        "set node.self to node\n",
        "node.self.self\n",
    );

    assert_eq!(
        eval(source),
        Value::Nothing,
        "`node.self` must hold the pre-write record, whose `self` was nothing: \
         an alias or a cycle would make this field the record itself"
    );
}

/// `MEMORY.md`: "there is no cycle collection".
///
/// Asserted against the tree rather than against the document, so a collector
/// cannot be added without this failing. The reach is deliberately wider than
/// the words "there is no `Weak`": a collector written against `Arc` itself
/// needs no `Weak` type at all, only `Arc::downgrade` to make one, so the
/// operations that build and consult a back-reference are named too, together
/// with the dependency check — a `gc` crate would add collection to the whole
/// program without a line of `src/` changing. `Rc` is allowed outside the value
/// representation — the parser's AST and the REPL's editor sessions use it — so
/// that check is scoped to `src/value.rs`, and the rest covers the whole crate.
#[test]
fn no_cycle_collector_exists_anywhere_in_the_crate() {
    let mut files = Vec::new();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    collect_rust_files(&src, &mut files);
    assert!(!files.is_empty(), "src/ should contain Rust source files");

    for file in &files {
        let relative = file
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(file)
            .display()
            .to_string();
        let text = std::fs::read_to_string(file).expect("source file should be readable");
        for forbidden in [
            "Weak",
            "WeakRef",
            "Gc",
            "gc::",
            "weak_count",
            "::downgrade",
            "::upgrade",
            "strong_count",
            "ptr_eq",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} mentions `{}`: there is no cycle collector, so nothing \
                 in the crate may reach for one",
                relative,
                forbidden
            );
        }
    }

    let value_rs = std::fs::read_to_string(src.join("value.rs")).expect("value.rs readable");
    assert!(
        !value_rs.contains("std::rc::Rc") && !value_rs.contains("Rc::"),
        "src/value.rs must not hold a `Rc`: a value that outlives its scope \
         cannot be dropped when the count reaches zero"
    );

    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("Cargo.toml should be readable");
    for collector in [
        "gc",
        "gc-arena",
        "weak",
        "crossbeam-epoch",
        "shred",
        "triomphe",
        "munge",
    ] {
        assert!(
            !manifest.contains(&format!("\"{collector}\"")),
            "Cargo.toml depends on `{collector}`: a tracing or epoch collector \
             would reclaim cycles without a word of it appearing in `src/`, and \
             MEMORY.md's limitation would quietly become untrue"
        );
    }
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} should be readable: {}", dir.display(), error));
    for entry in entries {
        let entry = entry.expect("directory entry should be readable");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// `MEMORY.md`: deep recursion raises a *catchable* `Error::Limit`.
///
/// The claim has two halves and both are checked here: the error is a `Limit`
/// rather than an abort, and a program can catch it and carry on. The lock is
/// held across both, so the limit this recursion runs under is the generated
/// one and not whatever an override test is setting at the time.
#[test]
fn deep_recursion_raises_a_catchable_limit_not_a_stack_overflow() {
    let _guard = exclusive();
    let err = run_err(&parse("to boom(n)\n    boom(n + 1)\nend\n\nboom(0)\n"));

    assert!(
        matches!(err, redblue::Error::Limit(_, _)),
        "unbounded recursion should raise Error::Limit, got {:?}",
        err
    );
    assert!(
        err.is_resource_limit(),
        "the error should report itself as a limit, got {:?}",
        err
    );
    assert!(
        !err.to_string().contains("stack overflow"),
        "the native stack was exhausted instead of the counter stopping it: {:?}",
        err
    );
    assert!(
        err.to_string().contains(&format!(
            "Maximum call depth of {} ",
            redblue::MAX_CALL_DEPTH
        )),
        "with {variable} unset the limit should be the generated one",
        variable = redblue::MAX_CALL_DEPTH_ENV
    );

    let caught = concat!(
        "to boom(n)\n",
        "    boom(n + 1)\n",
        "end\n",
        "set reached to no\n",
        "try\n",
        "    boom(0)\n",
        "catch error\n",
        "    set reached to yes\n",
        "end\n",
        "expect reached to be yes\n",
    );
    let (_vm, result) = redblue::run_isolated(&parse(caught));
    assert!(
        result.is_ok(),
        "a depth error must be catchable by `try ... catch error`, got {:?}",
        result.err()
    );
}

/// Two functions calling each other, which is the shortest program that can
/// reach any call-depth limit: one function's frame is still live when the
/// other's is entered, so the counter never unwinds.
const MUTUAL_RECURSION: &str = concat!(
    "to ping(n)\n",
    "    pong(n + 1)\n",
    "end\n",
    "to pong(n)\n",
    "    ping(n + 1)\n",
    "end\n",
    "ping(0)\n",
);

/// The same mutual recursion, bounded so that it finishes on its own.
///
/// A limit set in process is counted on whichever thread called [`Vm::run`] —
/// the interpreter sizes a thread only for the programs it starts itself — so a
/// program that would recurse to the limit is only safe here while the limit is
/// small. Bounding it turns "the override was ignored" into a program that
/// returns normally, which a test can report, instead of a stack overflow, which
/// it cannot.
const BOUNDED_RECURSION: &str = concat!(
    "to ping(n)\n",
    "    if n < 20 then\n",
    "        pong(n + 1)\n",
    "    end\n",
    "end\n",
    "to pong(n)\n",
    "    ping(n + 1)\n",
    "end\n",
    "ping(0)\n",
);

/// The limit the call-depth error names for an already-parsed `program`.
///
/// The limit is *in* the message — "Maximum call depth of N reached while
/// calling 'f'" — so this is what a test can read to find out which limit the
/// VM was actually running under. Asserting on the message rather than on a
/// counter is the point: an override that is resolved and then ignored leaves
/// the default in the message, and only reading the message catches that.
#[track_caller]
fn limit_named_by(program: &redblue::parser::Program) -> String {
    let message = run_err(program).to_string();
    message
        .split("Maximum call depth of ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| {
            panic!("the limit error should name the limit it reached, got {message:?}")
        })
        .to_string()
}

/// `MEMORY.md`: the limit is `REDBLUE_MAX_CALL_DEPTH`-overridable.
///
/// Three claims, all checked against the real call path rather than against the
/// constant's text: `resolve_max_call_depth()` reads the variable and falls back
/// to `MAX_CALL_DEPTH` for an absent, non-numeric, zero or negative value;
/// `run_isolated` hands what it resolved to the VM that runs; and
/// `Vm::with_max_call_depth` sets the same limit in process with the variable
/// ignored. A broken override leaves the default in the message, which is what
/// reading the message catches.
#[test]
fn the_call_depth_limit_has_the_documented_environment_override() {
    let variable = redblue::MAX_CALL_DEPTH_ENV;
    let program = parse(MUTUAL_RECURSION);
    let default = redblue::MAX_CALL_DEPTH.to_string();

    let _guard = exclusive();

    assert_eq!(
        variable, "REDBLUE_MAX_CALL_DEPTH",
        "MEMORY.md names this variable"
    );

    // Nothing set: the documented fallback is the generated limit.
    std::env::remove_var(variable);
    assert_eq!(
        limit_named_by(&program),
        default,
        "with {variable} unset the limit is MAX_CALL_DEPTH"
    );

    // Set: `run_isolated` resolves it and the VM counts against what it got.
    std::env::set_var(variable, "7");
    assert_eq!(
        limit_named_by(&program),
        "7".to_string(),
        "{variable}=7 must bound the call chain rather than the default"
    );

    // Padded, as an operator writing it into a shell would leave it.
    std::env::set_var(variable, "  12  ");
    assert_eq!(
        limit_named_by(&program),
        "12".to_string(),
        "{variable} is trimmed before it is read"
    );

    // The three refusals the document names, each of which falls back. A limit
    // of zero would make every function call illegal, which is never what an
    // operator means.
    for rejected in ["not a number", "0", "-3"] {
        std::env::set_var(variable, rejected);
        assert_eq!(
            limit_named_by(&program),
            default,
            "{variable}={rejected:?} is not a usable limit, so the default applies"
        );
    }
    std::env::remove_var(variable);

    // In process, with the variable ignored entirely. The program stops on its
    // own at twenty frames, so a limit that was set too high lets it finish
    // rather than recursing until the stack is gone: a wrong answer here is an
    // `expect_err` that panics, not an abort.
    std::env::set_var(variable, "2000");
    let mut vm = redblue::Vm::with_max_call_depth(9);
    let message = vm
        .run(&parse(BOUNDED_RECURSION))
        .expect_err("nine frames is not enough for twenty frames of recursion")
        .to_string();
    assert!(
        message.contains("Maximum call depth of 9 "),
        "Vm::with_max_call_depth must set the limit without reading {variable}, \
         and {variable} was 2000; got {message:?}"
    );

    // A zero is ignored rather than obeyed, for the same reason the variable's
    // zero is. A call that has to succeed is what shows it was ignored: a depth
    // error here would mean the zero was obeyed, and every call was illegal.
    let mut vm = redblue::Vm::with_max_call_depth(0);
    let outcome = vm.run(&parse("to f(n)\n    return n + 1\nend\nf(1)\n"));
    assert_eq!(
        outcome.expect("a zero limit must not make every call illegal"),
        Value::Number(2.0),
        "Vm::with_max_call_depth(0) must fall back to MAX_CALL_DEPTH"
    );

    std::env::remove_var(variable);
}

/// `MEMORY.md`: a property write goes through `Arc::make_mut`, on a record and
/// on nothing else.
///
/// The document used to call one line "the shape of every property write", which
/// is not true of either engine: both match `Value::Record` and silently leave
/// anything else alone, so a write against a number is dropped rather than
/// reported. And no program can produce the other container that carries fields,
/// `Value::Object` — an `object` declaration binds a `Record` — so there is no
/// third property-write shape to cover.
#[test]
fn a_property_write_reaches_a_record_and_no_program_can_make_an_object() {
    assert_eq!(
        Value::object("Node".to_string(), Default::default()).type_name(),
        "object",
        "`Value::Object` is a variant a program cannot build, which is why no \
         property write has to handle one"
    );
    assert_eq!(
        eval("object Node\n    has value\nend\nset n to Node\nn\n").type_name(),
        "record",
        "an `object` declaration binds a `Record`, so the `Object` variant is \
         unreachable from Redblue"
    );
    assert_eq!(
        eval("set x to 5\nset x.a to 1\nx\n"),
        Value::Number(5.0),
        "a property write whose target is not a record is silently left alone \
         rather than reported, on both engines"
    );

    for (file, guard, write) in [
        ("src/interpreter.rs", 1625, 1626),
        ("src/bytecode/vm.rs", 2664, 2665),
    ] {
        assert!(
            source_line(file, guard).contains("if let Some(Value::Record(mut fields))"),
            "{}:{} should be the guard that admits only a record",
            file,
            guard
        );
        assert!(
            source_line(file, write).contains("make_mut(&mut fields).insert("),
            "{}:{} should be the copy-on-write field write",
            file,
            write
        );
    }
}

/// Every `file:line` `MEMORY.md` cites must still say what the document claims.
///
/// This is the test that makes the rest of the file's citations worth anything.
#[test]
fn memory_md_citations_still_match_the_code() {
    let memory = memory_text();

    let shared = source_line("src/value.rs", 276);
    assert_eq!(
        shared.trim(),
        "pub type Shared<T> = std::sync::Arc<T>;",
        "src/value.rs:276 is the `Shared` alias MEMORY.md cites"
    );
    assert!(
        memory.contains("src/value.rs:276"),
        "MEMORY.md must cite src/value.rs:276 for the `Shared` alias"
    );

    for (line, variant) in [(253, "List"), (254, "Record"), (255, "Object")] {
        let text = source_line("src/value.rs", line);
        assert!(
            text.contains(&format!("{}(", variant)) && text.contains("Shared<"),
            "src/value.rs:{} should be the `Shared` {} variant, got {:?}",
            line,
            variant,
            text
        );
    }
    assert!(
        memory.contains("src/value.rs:253"),
        "MEMORY.md must cite the variant lines for the Arc-shared values"
    );

    let clone_rule = source_lines("src/value.rs")[87..90].join(" ");
    assert!(
        clone_rule.contains("reading any variable clones the value bound to"),
        "src/value.rs:88-90 states the copy-on-read rule, got {:?}",
        clone_rule
    );
    assert!(
        memory.contains("src/value.rs:88"),
        "MEMORY.md must cite src/value.rs:88 for the copy-on-read rule"
    );

    let vm_claim = source_lines("src/bytecode/vm.rs")[17..21].join(" ");
    assert!(
        vm_claim.contains("no path from a `.rbc` to a native stack overflow"),
        "src/bytecode/vm.rs:18-21 states the no-stack-overflow claim, got {:?}",
        vm_claim
    );
    assert!(
        memory.contains("src/bytecode/vm.rs"),
        "MEMORY.md must cite src/bytecode/vm.rs for the recursion bound"
    );

    // A function value's body and its captured scopes are two separate fields on
    // two separate lines, and the `Arc` each is behind is stated for the body
    // and declared on the field for the captures. A citation that covered only
    // the first would not support the second.
    assert!(
        source_lines("src/value.rs")[84..91]
            .join(" ")
            .contains("pub body: FunctionBody"),
        "src/value.rs:85-91 is the `body` field MEMORY.md cites"
    );
    assert_eq!(
        source_line("src/value.rs", 93).trim(),
        "pub captured: Arc<Captured>,",
        "src/value.rs:93 is where the captured scopes' `Arc` is declared"
    );
    assert!(
        memory.contains("src/value.rs:93"),
        "MEMORY.md must cite src/value.rs:93 for the captured scopes"
    );

    // The property-write claim covers both engines and both of them only reach a
    // `Record`, so the document has to name both sites rather than one of them
    // as the shape of every write.
    for citation in [
        "src/interpreter.rs:1619-1630",
        "src/bytecode/vm.rs:2652-2675",
    ] {
        assert!(
            memory.contains(citation),
            "MEMORY.md must cite {citation}, which is one of the two property \
             writes and the only two there are"
        );
    }
    let tree_arm = source_lines("src/interpreter.rs")[1618..1630].join(" ");
    assert!(
        tree_arm.contains("Statement::SetProperty")
            && tree_arm.contains("if let Some(Value::Record(mut fields))")
            && tree_arm.contains("Arc::make_mut(&mut fields).insert("),
        "src/interpreter.rs:1619-1630 is the tree-walking property write, and it \
         admits a record only"
    );
    let bytecode_write = source_lines("src/bytecode/vm.rs")[2651..2675].join(" ");
    assert!(
        bytecode_write.contains("fn set_property(")
            && bytecode_write.contains("if let Some(Value::Record(mut fields))")
            && bytecode_write.contains("Shared::make_mut(&mut fields).insert("),
        "src/bytecode/vm.rs:2652-2675 is the bytecode property write, and it \
         admits a record only"
    );

    assert_eq!(
        source_line("src/interpreter.rs", 42).trim(),
        "pub const MAX_CALL_DEPTH_ENV: &str = \"REDBLUE_MAX_CALL_DEPTH\";",
        "src/interpreter.rs:42 names the override variable MEMORY.md cites"
    );
    assert!(
        memory.contains("src/interpreter.rs:42"),
        "MEMORY.md must cite src/interpreter.rs:42 for the override variable"
    );
    assert!(
        source_line("src/interpreter.rs", 139).contains("pub fn resolve_max_call_depth()"),
        "src/interpreter.rs:139 is the resolver MEMORY.md cites, and it reads the \
         variable rather than ignoring it"
    );
    assert!(
        memory.contains("src/interpreter.rs:139"),
        "MEMORY.md must cite src/interpreter.rs:139 for the resolver"
    );
}

/// `MEMORY.md` has to exist, and `README.md` has to point at it — otherwise the
/// model is documented nowhere a user would look, which is the whole finding.
#[test]
fn memory_md_is_linked_from_the_readme() {
    let memory = memory_text();
    assert!(
        memory.contains("src/value.rs:276"),
        "MEMORY.md must state the value semantics and cite src/value.rs:276"
    );
    assert!(
        memory.to_lowercase().contains("cycle"),
        "MEMORY.md must name the cycle leak as a known limitation"
    );

    assert!(
        readme_text().contains("MEMORY.md"),
        "README.md must link MEMORY.md"
    );
}
