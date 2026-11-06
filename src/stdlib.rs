use crate::value::{Fields, FunctionValue, Value};
use std::collections::HashMap;

/// The module names whose functions are registered as `module_function`
/// builtins, so that a caller can tell `json.parse` — a module — from a
/// variable that happens to be followed by a `.`.
///
/// `text` and `math` are here because SPEC.md documents `text.uppercase` and
/// `math.sqrt`. They were missing, which made every dotted spelling of them fail
/// in the analyzer as `Unknown variable 'text'` — one step before the VM could
/// report a missing function, and naming the wrong thing.
///
/// `formats` is deliberately **not** here. SPEC.md § formats used to document
/// `formats.parse_json`, `formats.to_json` and `formats.parse_csv`, none of
/// which were ever names in the language; JSON and CSV are their own modules.
/// That section was corrected rather than the module invented, because adding
/// `formats` would make a misspelling resolve to a namespace and fail later and
/// less clearly than it does now.
pub const MODULES: &[&str] = &[
    "base64",
    "bytes",
    "collections",
    "console",
    "csv",
    "decimal",
    "files",
    "hash",
    "hex",
    "json",
    "list",
    "logging",
    "math",
    "matrix",
    "network",
    "path",
    "regex",
    "sys",
    "tar",
    "text",
    "time",
    "toml",
    "uuid",
    "zip",
];

/// The modules whose functions are the flat builtins of the same name rather
/// than a `<module>_<function>` entry in the registry: `text.uppercase` is
/// `uppercase`, and `math.sqrt` is `sqrt`.
///
/// A module listed here is *expected* to have no `<module>_<function>` builtin
/// behind it. Every other name in [`MODULES`] is expected to have one, which is
/// what [`modules_without_functions`] checks.
pub const ALIAS_MODULES: &[&str] = &["math", "text"];

/// Every function each module exposes, as the caller spells it.
///
/// This is what makes [`members_without_functions`] a per-member check rather
/// than a per-module one. The check it replaced asked whether *any* builtin
/// started with `<module>_`, which one stub answers for the whole module:
/// `list_map` made `list` look complete while `list.filter` and `list.reduce`
/// — two names nothing implements — counted as members of it, and dropping
/// `filter` from the registry entirely would have left the gate green. Every
/// member is named here, so a member the language has is a line in this table,
/// and a member that resolves to nothing is one name rather than a whole
/// namespace nobody looked at.
///
/// It is also the surface the language enforces rather than only documents, and
/// it is enforced on **both** of the paths a dotted call can take:
///
/// - a flat-backed member ([`module_member_name`]) unwraps `module_member` to
///   `member` only for a pair this table holds;
/// - a `<module>_<member>` entry in the registry is checked against the table
///   before the engines will reach it (`src/interpreter.rs`,
///   `src/bytecode/vm.rs`), so `files_read` being registered is not by itself a
///   reason `files.read` answers.
///
/// A member that is not named here is therefore refused by its dotted spelling
/// however many builtins the registry holds under that name, and a name in this
/// table is a decision — "this module has this member" — rather than a listing
/// of whatever happened to be reachable.
///
/// Every module in [`MODULES`] appears exactly once, including the two in
/// [`ALIAS_MODULES`]: their members are flat builtins, and spelling them
/// `<module>.<member>` is how a program reaches them.
pub const MODULE_MEMBERS: &[(&str, &[&str])] = &[
    ("base64", &["encode", "decode"]),
    ("bytes", &["from_text", "text", "write"]),
    (
        "collections",
        &[
            "counter",
            "count",
            "most_common",
            "defaultdict",
            "get",
            "put",
            "deque",
            "push_left",
            "push_right",
            "pop_left",
            "pop_right",
            "namedtuple",
            "field",
        ],
    ),
    ("console", &["log", "error", "clear"]),
    (
        "logging",
        &[
            "debug",
            "info",
            "warn",
            "error",
            "set_level",
            "level",
            "set_timestamps",
            "set_destination",
            "reset",
        ],
    ),
    ("csv", &["parse", "stringify"]),
    (
        "files",
        &[
            "read", "write", "append", "exists", "lines", "delete", "copy", "rename",
        ],
    ),
    ("hash", &["sha256", "sha512"]),
    ("hex", &["encode", "decode"]),
    (
        "json",
        &["parse", "stringify", "parse_lines", "stringify_lines"],
    ),
    // `filter` and `reduce` are **not** here, and leaving them out is what
    // `list.filter(xs, f)` needs rather than a detail. Both are registered as
    // flat builtins and neither is implemented — `tests/stdlib_module_docs_test.rs`
    // carries them in `known_gaps()` with the program that still fails — so a
    // dotted spelling of one reached a dispatcher that refuses every call: a name
    // the analyzer accepted and the runtime could only ever refuse. That is the
    // same position as `math.random`, which SPEC.md also documents as
    // "not implemented" and which `math.random(1, 6)` refuses by name. A builtin
    // that is not implemented is not a member of a module.
    //
    // `length` is here for the same reason `map` is: it is a flat builtin with a
    // dotted spelling, and `tests/text_split_join_test.rs` pins `list.length`
    // against `length` for both. It answers a list and a text — and refuses a
    // record, as `runtime::builtin` has always done and as SPEC.md § text now
    // writes down — so it is named by the two modules whose members it is a
    // member of: see the same name under `text`. Both documents write the
    // spelling, which is what `tests/stdlib_module_names_test.rs` checks in
    // both directions.
    ("list", &["map", "length"]),
    (
        "math",
        &[
            "abs",
            "floor",
            "ceil",
            "round",
            "sqrt",
            "pow",
            "sin",
            "cos",
            "tan",
            "log",
            "exp",
            "mean",
            "median",
            "mode",
            "variance",
            "standard_deviation",
            "percentile",
            "correlation",
        ],
    ),
    (
        "decimal",
        &["from_text", "from_number", "to_text", "to_number", "sum"],
    ),
    (
        "matrix",
        &[
            "zeros",
            "ones",
            "identity",
            "from_lists",
            "to_lists",
            "shape",
            "add",
            "subtract",
            "multiply",
            "divide",
            "scale",
            "matmul",
            "transpose",
            "sum",
            "mean",
            "min",
            "max",
            "at",
            "row",
            "column",
        ],
    ),
    ("network", &["get", "post"]),
    (
        "path",
        &[
            "join",
            "parent",
            "filename",
            "stem",
            "extension",
            "is_absolute",
            "resolve",
            "normalize",
            "list",
        ],
    ),
    (
        "regex",
        &["match", "is_match", "capture", "replace", "split"],
    ),
    ("sys", &["argv", "parse"]),
    (
        "text",
        &[
            "uppercase",
            "lowercase",
            "trim",
            "split",
            "join",
            "contains",
            "starts_with",
            "ends_with",
            "replace",
            "wrap",
            "diff",
            "dedent",
            "template",
            "pad",
            "align",
            // `length` is registered flat and both documents write it both ways —
            // SPEC.md § text and README.md § Text and § List — so `text.length`
            // and `list.length` are spellings a program may write. Naming them is
            // what keeps them: an unnamed spelling of a registered builtin was not
            // a decision in this table but an accident of how far the unwrapping
            // reached, and a program that found one had no way to know it was
            // reaching a function no document lists.
            "length",
        ],
    ),
    ("toml", &["parse"]),
    ("uuid", &["generate", "parse"]),
    ("zip", &["read", "write"]),
    ("tar", &["read", "write"]),
    (
        "time",
        &[
            "now",
            "sleep",
            "format",
            "parse",
            "unix",
            "from_unix",
            "date",
            "datetime",
            "duration",
            "add_years",
            "add_months",
            "add_days",
            "add_hours",
            "add_minutes",
            "add_seconds",
        ],
    ),
];

/// Whether `module.member` is one of the spellings the language has.
///
/// A member is named in [`MODULE_MEMBERS`], which is the whole of a **stdlib**
/// module's surface: a name that is not in that row is not in the language,
/// whatever the registry happens to hold. This is the predicate both engines
/// resolve a dotted call against — the flat-backed path through
/// [`module_member_name`] and the `<module>_<member>` path, which both refuse
/// first — so a builtin cannot answer as a member of one engine and be refused
/// by the other, and cannot answer at all for a pair this table does not hold.
///
/// It says nothing about a module a *program* declared or imported: those are
/// namespaces the program built, and the rule that applies to them is
/// [`dotted_call_reaches`] rather than this table.
pub fn module_has_member(module: &str, member: &str) -> bool {
    MODULE_MEMBERS
        .iter()
        .any(|(listed, members)| *listed == module && members.contains(&member))
}

/// The members of `module`, as a caller spells them, or `None` when `module` is
/// not one of the [`MODULES`].
///
/// The row of [`MODULE_MEMBERS`] itself rather than a second list: the REPL
/// offers what this returns, so a member added to the language is completable
/// because the same table the analyzer enforces it against is what the completer
/// reads.
pub fn module_members(module: &str) -> Option<&'static [&'static str]> {
    MODULE_MEMBERS
        .iter()
        .find(|(listed, _)| *listed == module)
        .map(|(_, members)| *members)
}

/// Whether a dotted call on a receiver that is not an object reaches the function
/// it names.
///
/// This is the whole rule, in one place, because both engines ask it before they
/// resolve anything — [`crate::interpreter::Vm::call_method`] and
/// [`crate::bytecode::vm::BytecodeVm::call_method`] — and a rule written twice is
/// a rule the next change will apply to one of them.
///
/// Three questions, in the order a call asks them, and the first that says no is
/// the refusal. The two middle facts are about this particular program and only
/// the engine running it knows them; the rest is decided here.
///
/// 1. **Does the receiver name a module at all?** A receiver that is not an
///    object has to be one. Being a *registered builtin* is a different thing:
///    `random` is a builtin name, and `random.seed(1)` used to answer through
///    `random_seed` — because `<receiver>_<member>` being bound was on its own
///    enough to make the call, every `x_y` in the registry was a module called
///    `x`. `PI` is a builtin name too, and `PI.times(..)` is a call on a number.
/// 2. **Does the program's own module win?** A module it declared with
///    `module … end`, or imported from a file, owns the name outright: its
///    members are the ones it published, and neither this crate's [`MODULES`] nor
///    [`MODULE_MEMBERS`] says anything about them. A `module text … end` that
///    publishes `greet` is the program saying what `text` means here, and
///    refusing it as `Module 'text' has no function 'greet'` is this crate
///    deciding a question it was not asked. Declaring over a module of the same
///    name is the rule an object already follows — `tests/object_model_test.rs`
///    pins `object_named_like_a_module_takes_the_call` — so it is the same
///    answer here.
/// 3. **Is the member one the stdlib holds?** For a stdlib module this is
///    [`module_has_member`], and the pair additionally has to be bound: named in
///    the registry under `<module>_<member>`, or flat-backed through
///    [`module_member_name`]. Enumerating the members says which spellings exist;
///    it does not make them answer.
pub fn dotted_call_reaches(
    module: &str,
    member: &str,
    declared_by_the_program: bool,
    bound_by_the_program: bool,
    answers_as_a_builtin: bool,
) -> bool {
    if declared_by_the_program {
        return bound_by_the_program;
    }
    if !is_module(module) {
        return false;
    }
    module_has_member(module, member) && (bound_by_the_program || answers_as_a_builtin)
}

/// The function a method-spelled `map` call was given, or the refusal it earns.
///
/// `map` is the one builtin whose argument has to be a Redblue function, so it
/// is the one call whose bad arguments are named rather than merely counted:
/// `xs.map(5)` says what it got, and a call with no argument at all says that
/// too. Both engines ask this — see [`crate::interpreter::Vm::call_method`] and
/// [`crate::bytecode::vm::BytecodeVm::call_method`] — because the two engines
/// refusing with different words is a divergence `tests/differential_test.rs`
/// reports for a program that is behaving the same way.
///
/// The caller has already established that the receiver is a list: this is the
/// argument side alone, and a receiver that is not a list is refused by the
/// receiver path rather than here.
pub fn map_function(args: &[Value]) -> Result<&FunctionValue, String> {
    match args {
        [Value::Function(function)] => Ok(function),
        [] => Err("map requires a function, but no argument was given".to_string()),
        [one] => Err(format!("map requires a function, but was given {one}")),
        many => Err(format!(
            "map requires a function, but was given {} arguments",
            many.len()
        )),
    }
}

/// Whether the `module.member` spelling reaches a builtin.
///
/// Two ways, and which one applies is not a choice the caller makes. A
/// `<module>_<member>` entry in the registry is already the right name — that is
/// the internal encoding `files.read` is stored under — so it answers as itself.
/// Anything else is resolved through [`module_member_name`], which is how the
/// two [`ALIAS_MODULES`] (`text.uppercase` is `uppercase`) and how `list.length`
/// reaches the flat `length` reach their builtin at all.
///
/// Reachability is not membership: this says a pair *can* answer, and
/// [`module_has_member`] says whether the language has the spelling at all. Both
/// are needed, and the engines ask the second one first.
pub fn member_resolves(module: &str, member: &str) -> bool {
    let qualified = format!("{module}_{member}");
    BUILTIN_NAMES.contains(&qualified.as_str()) || module_member_name(&qualified).is_some()
}

/// The `module.member` pairs in [`MODULE_MEMBERS`] that reach no builtin.
///
/// A name in that position is a spelling the analyzer accepts — the receiver is
/// a module — and the runtime then refuses, which is worse than either error
/// alone because the program passes analysis and dies on its first call.
/// Should always be empty.
pub fn members_without_functions() -> Vec<String> {
    MODULE_MEMBERS
        .iter()
        .flat_map(|(module, members)| {
            members
                .iter()
                .filter(|member| !member_resolves(module, member))
                .map(move |member| format!("{module}.{member}"))
        })
        .collect()
}

/// Whether `name` is one of the [`MODULES`].
pub fn is_module(name: &str) -> bool {
    MODULES.contains(&name)
}

/// The names in [`MODULES`] with no function behind them and no entry in
/// [`ALIAS_MODULES`] to explain why.
///
/// A name in that position is a namespace the analyzer accepts as the receiver
/// of a dotted call and the runtime then refuses — a worse failure than either
/// error alone, because the program passes analysis and dies on its first call.
/// Should always be empty.
///
/// Derived from [`MODULE_MEMBERS`] rather than from a prefix scan over the
/// registry, so it and [`members_without_functions`] cannot disagree about what
/// a module has behind it.
pub fn modules_without_functions() -> Vec<&'static str> {
    MODULES
        .iter()
        .copied()
        .filter(|name| !ALIAS_MODULES.contains(name))
        .filter(|name| {
            MODULE_MEMBERS
                .iter()
                .find(|(module, _)| module == name)
                .is_some_and(|(_, members)| {
                    !members.is_empty()
                        && members.iter().all(|member| !member_resolves(name, member))
                })
        })
        .collect()
}

pub fn builtins() -> HashMap<String, Value> {
    let mut globals = HashMap::new();

    // Math constants
    globals.insert("PI".to_string(), Value::Number(std::f64::consts::PI));
    globals.insert("E".to_string(), Value::Number(std::f64::consts::E));

    // Math functions
    globals.insert("abs".to_string(), Value::Builtin("abs".to_string()));
    globals.insert("floor".to_string(), Value::Builtin("floor".to_string()));
    globals.insert("ceil".to_string(), Value::Builtin("ceil".to_string()));
    globals.insert("round".to_string(), Value::Builtin("round".to_string()));
    globals.insert("sqrt".to_string(), Value::Builtin("sqrt".to_string()));
    globals.insert("pow".to_string(), Value::Builtin("pow".to_string()));
    globals.insert("sin".to_string(), Value::Builtin("sin".to_string()));
    globals.insert("cos".to_string(), Value::Builtin("cos".to_string()));
    globals.insert("tan".to_string(), Value::Builtin("tan".to_string()));
    globals.insert("log".to_string(), Value::Builtin("log".to_string()));
    globals.insert("exp".to_string(), Value::Builtin("exp".to_string()));
    for name in [
        "mean",
        "median",
        "mode",
        "variance",
        "standard_deviation",
        "percentile",
        "correlation",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }
    for name in [
        "decimal_from_text",
        "decimal_from_number",
        "decimal_to_text",
        "decimal_to_number",
        "decimal_sum",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }

    // Text functions
    globals.insert(
        "uppercase".to_string(),
        Value::Builtin("uppercase".to_string()),
    );
    globals.insert(
        "lowercase".to_string(),
        Value::Builtin("lowercase".to_string()),
    );
    globals.insert("trim".to_string(), Value::Builtin("trim".to_string()));
    globals.insert("split".to_string(), Value::Builtin("split".to_string()));
    globals.insert("join".to_string(), Value::Builtin("join".to_string()));
    globals.insert(
        "contains".to_string(),
        Value::Builtin("contains".to_string()),
    );
    globals.insert(
        "starts_with".to_string(),
        Value::Builtin("starts_with".to_string()),
    );
    globals.insert(
        "ends_with".to_string(),
        Value::Builtin("ends_with".to_string()),
    );
    globals.insert("replace".to_string(), Value::Builtin("replace".to_string()));

    // List functions
    globals.insert("length".to_string(), Value::Builtin("length".to_string()));
    globals.insert("push".to_string(), Value::Builtin("push".to_string()));
    globals.insert("append".to_string(), Value::Builtin("append".to_string()));
    globals.insert("pop".to_string(), Value::Builtin("pop".to_string()));
    globals.insert("shift".to_string(), Value::Builtin("shift".to_string()));
    globals.insert("map".to_string(), Value::Builtin("map".to_string()));
    globals.insert("filter".to_string(), Value::Builtin("filter".to_string()));
    globals.insert("reduce".to_string(), Value::Builtin("reduce".to_string()));
    // The iteration helpers. They are flat builtins rather than members of
    // `list`, because they are not list methods and phase-077 owns that surface.
    for name in [
        "chain",
        "zip",
        "enumerate",
        "product",
        "groupby",
        "take",
        "skip",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }
    // Collections module: a counter, a defaultdict, a deque and named records.
    // Every value is an existing Value variant — a record of pairs, a pair of
    // linked-list records, a record of named fields — so no new kind of value
    // enters the language.
    for name in [
        "collections_counter",
        "collections_count",
        "collections_most_common",
        "collections_defaultdict",
        "collections_get",
        "collections_put",
        "collections_deque",
        "collections_push_left",
        "collections_push_right",
        "collections_pop_left",
        "collections_pop_right",
        "collections_namedtuple",
        "collections_field",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }
    // `list.map`, the module spelling SPEC.md § list writes for the same
    // builtin. It is a higher-order call, so the VM resolves it rather than
    // `runtime::builtin`.
    globals.insert(
        "list_map".to_string(),
        Value::Builtin("list_map".to_string()),
    );

    // Type checking
    globals.insert(
        "is_number".to_string(),
        Value::Builtin("is_number".to_string()),
    );
    globals.insert("is_text".to_string(), Value::Builtin("is_text".to_string()));
    globals.insert("is_list".to_string(), Value::Builtin("is_list".to_string()));
    globals.insert(
        "is_record".to_string(),
        Value::Builtin("is_record".to_string()),
    );

    // Conversion
    globals.insert("to_text".to_string(), Value::Builtin("to_text".to_string()));
    globals.insert(
        "to_number".to_string(),
        Value::Builtin("to_number".to_string()),
    );
    globals.insert("to_list".to_string(), Value::Builtin("to_list".to_string()));

    // Files module
    globals.insert(
        "files_read".to_string(),
        Value::Builtin("files_read".to_string()),
    );
    globals.insert(
        "files_write".to_string(),
        Value::Builtin("files_write".to_string()),
    );
    globals.insert(
        "files_append".to_string(),
        Value::Builtin("files_append".to_string()),
    );
    globals.insert(
        "files_exists".to_string(),
        Value::Builtin("files_exists".to_string()),
    );
    globals.insert(
        "files_lines".to_string(),
        Value::Builtin("files_lines".to_string()),
    );
    globals.insert(
        "files_delete".to_string(),
        Value::Builtin("files_delete".to_string()),
    );
    globals.insert(
        "files_copy".to_string(),
        Value::Builtin("files_copy".to_string()),
    );
    globals.insert(
        "files_rename".to_string(),
        Value::Builtin("files_rename".to_string()),
    );

    // Path module: the host's path rules rather than string concatenation.
    for name in [
        "path_join",
        "path_parent",
        "path_filename",
        "path_stem",
        "path_extension",
        "path_is_absolute",
        "path_resolve",
        "path_normalize",
        "path_list",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }

    // Time module
    globals.insert(
        "time_now".to_string(),
        Value::Builtin("time_now".to_string()),
    );
    globals.insert(
        "time_sleep".to_string(),
        Value::Builtin("time_sleep".to_string()),
    );
    globals.insert(
        "time_format".to_string(),
        Value::Builtin("time_format".to_string()),
    );
    globals.insert(
        "time_unix".to_string(),
        Value::Builtin("time_unix".to_string()),
    );
    for name in [
        "time_parse",
        "time_from_unix",
        "time_date",
        "time_datetime",
        "time_duration",
        "time_add_years",
        "time_add_months",
        "time_add_days",
        "time_add_hours",
        "time_add_minutes",
        "time_add_seconds",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }

    // Formats module
    globals.insert(
        "json_parse".to_string(),
        Value::Builtin("json_parse".to_string()),
    );
    globals.insert(
        "json_stringify".to_string(),
        Value::Builtin("json_stringify".to_string()),
    );
    globals.insert(
        "csv_parse".to_string(),
        Value::Builtin("csv_parse".to_string()),
    );
    for name in [
        "csv_stringify",
        "json_parse_lines",
        "json_stringify_lines",
        "toml_parse",
        "zip_read",
        "zip_write",
        "tar_read",
        "tar_write",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }

    // Matrix module: a dense numeric array. The kernels are `ndarray`'s, not
    // loops written in Redblue, because dense numeric work is exactly what an
    // interpreter loses on; LIBRARIES.md § 2 requires the binding.
    for name in [
        "matrix_zeros",
        "matrix_ones",
        "matrix_identity",
        "matrix_from_lists",
        "matrix_to_lists",
        "matrix_shape",
        "matrix_add",
        "matrix_subtract",
        "matrix_multiply",
        "matrix_divide",
        "matrix_scale",
        "matrix_matmul",
        "matrix_transpose",
        "matrix_sum",
        "matrix_mean",
        "matrix_min",
        "matrix_max",
        "matrix_at",
        "matrix_row",
        "matrix_column",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }

    // Network module
    globals.insert(
        "network_get".to_string(),
        Value::Builtin("network_get".to_string()),
    );
    globals.insert(
        "network_post".to_string(),
        Value::Builtin("network_post".to_string()),
    );

    // Regex module. The matching itself is the `regex` crate rather than an
    // engine written here: LIBRARIES.md § 2 names it as a binding, and the
    // crate is already linked for the test harness.
    for name in [
        "regex_match",
        "regex_is_match",
        "regex_capture",
        "regex_replace",
        "regex_split",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }

    // Testing module
    globals.insert("expect".to_string(), Value::Builtin("expect".to_string()));
    globals.insert("assert".to_string(), Value::Builtin("assert".to_string()));

    // Console module
    globals.insert(
        "console_log".to_string(),
        Value::Builtin("console_log".to_string()),
    );
    globals.insert(
        "console_error".to_string(),
        Value::Builtin("console_error".to_string()),
    );
    globals.insert(
        "console_clear".to_string(),
        Value::Builtin("console_clear".to_string()),
    );

    // Random module
    globals.insert("random".to_string(), Value::Builtin("random".to_string()));
    globals.insert(
        "random_seed".to_string(),
        Value::Builtin("random_seed".to_string()),
    );
    globals.insert(
        "random_number".to_string(),
        Value::Builtin("random_number".to_string()),
    );
    globals.insert(
        "random_choice".to_string(),
        Value::Builtin("random_choice".to_string()),
    );
    globals.insert(
        "random_shuffle".to_string(),
        Value::Builtin("random_shuffle".to_string()),
    );

    // Type conversion
    globals.insert("type_of".to_string(), Value::Builtin("type_of".to_string()));

    // Bytes module: the binary file API. `files` writes text, so without this a
    // program cannot write a byte it cannot spell — which is most of a bytecode
    // file.
    globals.insert(
        "bytes_from_text".to_string(),
        Value::Builtin("bytes_from_text".to_string()),
    );
    globals.insert(
        "bytes_write".to_string(),
        Value::Builtin("bytes_write".to_string()),
    );
    globals.insert(
        "bytes_text".to_string(),
        Value::Builtin("bytes_text".to_string()),
    );

    // Encodings, digests and identifiers. The algorithms are the crates'
    // (`base64`, `hex`, `sha2`, `uuid`), not versions written in Redblue: a
    // hand-rolled digest is slow and easy to get wrong, and the policy in
    // LIBRARIES.md requires a binding for this class.
    for name in [
        "base64_encode",
        "base64_decode",
        "hex_encode",
        "hex_decode",
        "hash_sha256",
        "hash_sha512",
        "uuid_generate",
        "uuid_parse",
    ] {
        globals.insert(name.to_string(), Value::Builtin(name.to_string()));
    }

    // Sys module
    globals.insert(
        "sys_argv".to_string(),
        Value::Builtin("sys_argv".to_string()),
    );

    // Input. `runtime::builtin` has answered `input` since phase-028, and it
    // answered it *reachably*: a callee written with parentheses is resolved at
    // run time and never asked of the analyzer, so `input("n")` read stdin while
    // this table did not hold the name — which is how SPEC.md came to say the
    // language reads nothing from the program it is running in, and how
    // `edge_no_registered_builtin_is_unreachable_from_a_program` could not see
    // it. Registering it makes the registry and the language one set.
    //
    // `ask` is deliberately not registered: it is a freed keyword, and
    // `tests/keyword_reservation_test.rs` holds it as an ordinary identifier.
    globals.insert("input".to_string(), Value::Builtin("input".to_string()));

    globals
}

/// The non-function globals `builtins()` registers.
///
/// `PI` and `E` are values rather than callables, so `is_builtin` — which
/// answers for names a program can *call* — does not cover them, and the
/// analyzer needs to know they are in scope.
pub const GLOBAL_CONSTANTS: &[&str] = &["PI", "E"];

/// Whether `name` is one of the [`GLOBAL_CONSTANTS`].
pub fn is_global(name: &str) -> bool {
    GLOBAL_CONSTANTS.contains(&name)
}

/// Calls the builtin `name`, for the builtins [`runtime::builtin`] does not
/// answer.
///
/// This function used to be **dead code**: both VMs asked `runtime::builtin`,
/// which matched only the names it implemented, so `abs`, `floor`, `ceil`,
/// `round`, `sqrt`, `uppercase`, `lowercase` and `trim` — all registered in
/// [`builtins`], all in SPEC.md and README.md — answered `Unknown function` at
/// runtime. The only callers of this function were tests, and a test calling it
/// directly is green whether or not a program can reach it.
///
/// It is now reached, through [`builtin`], which is the single place a builtin
/// name is resolved and is called by both engines.
///
/// `None` means "wrong argument type or wrong argument count", and
/// `Some(Value::Nothing)` means "the builtin ran and has no answer" — `sqrt` of
/// a negative number is the last case. It has no real answer, and handing back
/// `NaN` would put a value into `Value::Number` that no comparison and no
/// display can be trusted on.
pub fn builtin_function(name: &str, args: Vec<Value>) -> Option<Value> {
    match name {
        // Math functions
        "abs" => Some(args.first()?.abs()),
        "floor" => Some(args.first()?.floor()),
        "ceil" => Some(args.first()?.ceil()),
        "round" => Some(args.first()?.round()),
        // `sqrt` of a negative answers `nothing`, which is the contract
        // `numeric_edge_test.rs` pins and what SPEC.md states: a negative has no
        // real root, and `nothing` is a value the language can test for, where
        // `NaN` would be a `Value::Number` no comparison and no display can be
        // trusted on. It is *not* an error — the caller asked a well-formed
        // question and the answer is that there is no such number.
        "sqrt" => match args.first()? {
            Value::Number(n) => {
                let root = n.sqrt();
                if root.is_finite() {
                    Some(Value::Number(root))
                } else {
                    Some(Value::Nothing)
                }
            }
            _ => None,
        },
        "pow" => match (args.first()?, args.get(1)?) {
            (Value::Number(base), Value::Number(exponent)) => {
                let result = base.powf(*exponent);
                // A negative base with a fractional exponent has no real answer,
                // and an overflow has none that is finite. Both are refused the
                // same way `sqrt` refuses a negative, rather than becoming a
                // non-finite number no comparison can be trusted on.
                if result.is_finite() {
                    Some(Value::Number(result))
                } else {
                    Some(Value::Nothing)
                }
            }
            _ => None,
        },
        "sin" => Some(args.first()?.unary("sin")),
        "cos" => Some(args.first()?.unary("cos")),
        "tan" => Some(args.first()?.unary("tan")),
        "log" => match args.first()? {
            Value::Number(n) if *n > 0.0 => Some(Value::Number(n.ln())),
            _ => Some(Value::Nothing),
        },
        "exp" => match args.first()? {
            Value::Number(n) => {
                let result = n.exp();
                if result.is_finite() {
                    Some(Value::Number(result))
                } else {
                    Some(Value::Nothing)
                }
            }
            _ => None,
        },

        // Text functions
        "uppercase" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.to_uppercase()))
            } else {
                None
            }
        }
        "lowercase" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.to_lowercase()))
            } else {
                None
            }
        }
        "trim" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.trim().to_string()))
            } else {
                None
            }
        }
        "split" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(by)) => Some(Value::list(
                // An empty separator would make every position a boundary and
                // never advance, so it is the one input that has no answer here.
                // This is a literal separator: `regex.split` is the pattern
                // form, and `text.split` keeps its literal meaning.
                if by.is_empty() {
                    return Some(Value::Nothing);
                } else {
                    text.split(by.as_str())
                        .map(|part| Value::Text(part.to_string()))
                        .collect()
                },
            )),
            _ => None,
        },
        "join" => match (args.first()?, args.get(1)?) {
            (Value::List(items), Value::Text(by)) => Some(Value::Text(
                items
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
                    .join(by),
            )),
            _ => None,
        },
        // `contains` has two meanings in this language and both are load-bearing.
        // `contains(text, needle)` is the text search; `contains(list, value)` is
        // membership. The self-hosted compiler uses the list form on every
        // `skip_newlines` dispatch, so narrowing this to text broke the fixed point —
        // which is what caught it.
        "contains" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(needle)) => {
                Some(Value::YesNo(text.contains(needle.as_str())))
            }
            (Value::List(items), value) => Some(Value::YesNo(items.contains(value))),
            _ => None,
        },
        "starts_with" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(prefix)) => {
                Some(Value::YesNo(text.starts_with(prefix.as_str())))
            }
            _ => None,
        },
        "ends_with" => match (args.first()?, args.get(1)?) {
            (Value::Text(text), Value::Text(suffix)) => {
                Some(Value::YesNo(text.ends_with(suffix.as_str())))
            }
            _ => None,
        },
        "replace" => match (args.first()?, args.get(1)?, args.get(2)?) {
            (Value::Text(text), Value::Text(from), Value::Text(to)) if !from.is_empty() => {
                Some(Value::Text(text.replace(from.as_str(), to.as_str())))
            }
            _ => None,
        },

        // Type predicates
        "is_number" => Some(Value::YesNo(matches!(args.first()?, Value::Number(_)))),
        "is_text" => Some(Value::YesNo(matches!(args.first()?, Value::Text(_)))),
        "is_list" => Some(Value::YesNo(matches!(args.first()?, Value::List(_)))),
        "is_record" => Some(Value::YesNo(matches!(args.first()?, Value::Record(_)))),

        // `length` has no arm here, and that is the point. It had one, and it
        // was dead: [`crate::runtime::builtin`] matches `"length" | "len"`
        // before this function is consulted, and answers for both a list and a
        // text and returns a runtime error for anything else — so nothing ever
        // reached the copy below. Two implementations of one function that
        // could not see each other is how `length("日本")` was 6 in one and 2 in
        // the other for as long as both existed, with only the reachable one
        // ever running. There is now one arm, in `runtime::builtin`, and
        // `tests/text_length_characters_test.rs` fails if a second grows here.

        // `pop` and `shift` return the element they removed and leave the list they
        // were given as it was. Redblue's lists are copy-on-write, so a program
        // that wants the shorter list writes `set xs to pop(xs)` — and a
        // program that wants only the element gets only the element. An empty
        // list has nothing to remove, which is `Value::Nothing` rather than a
        // fabricated element.
        "pop" => match args.first()? {
            Value::List(items) => items.last().cloned(),
            _ => None,
        },
        "shift" => match args.first()? {
            Value::List(items) => items.first().cloned(),
            _ => None,
        },
        "to_list" => match args.first()? {
            Value::List(_) => Some(args[0].clone()),
            Value::Text(text) => Some(Value::list(
                text.split_whitespace()
                    .map(|word| Value::Text(word.to_string()))
                    .collect(),
            )),
            _ => None,
        },

        _ => None,
    }
}

/// The single place a builtin name is resolved.
///
/// `runtime::builtin` answers the builtins that can fail and need a span; this
/// answers the rest, which is what [`builtin_function`] implements. It is called
/// by both engines, so a builtin cannot answer in the tree-walker and be missing
/// from the bytecode VM.
///
/// `Ok(None)` means "no builtin of that name", which is what lets the caller
/// carry on to user functions and finally to `Unknown function`. A builtin that
/// *ran* and refused is `Err`, and it is never downgraded to "no such name":
/// `files.read` given a number it cannot read has to say so rather than claim
/// the function does not exist.
pub fn builtin(
    span: crate::error::Span,
    name: &str,
    args: &[Value],
) -> crate::error::Result<Option<Value>> {
    // A module member is answered under its bare builtin name, so `text.uppercase`
    // and `uppercase` are one function rather than two.
    let name = resolve(name);
    if let Some(value) = crate::runtime::builtin(span, name, args)? {
        return Ok(Some(value));
    }
    if matches!(
        name,
        "regex_match" | "regex_is_match" | "regex_capture" | "regex_replace" | "regex_split"
    ) {
        return regex_builtin(span, name, args).map(Some);
    }
    if let Some(value) = text_utility_builtin(span, name, args)? {
        return Ok(Some(value));
    }
    match builtin_function(name, args.to_vec()) {
        // `Value::Nothing` out of `builtin_function` means the builtin ran and
        // has no answer to give — `sqrt` of a negative, `log` of zero, an
        // overflowing `pow`. At the language boundary that is an error naming
        // the function, never a number and never a silent `nothing`: the reason
        // is that a `Value::Number` must never hold a value no comparison can
        // settle, and a program that asked a question the language cannot answer
        // is owed the name of what it asked.
        Some(Value::Nothing) => Err(crate::error::Error::Runtime(
            format!("{name} has no answer for these arguments"),
            span,
        )),
        Some(value) => Ok(Some(value)),
        // A builtin that recognised itself and could not use the arguments is
        // also an error by name. `Unknown function` is the message for a name
        // nothing implements, and applying it to a name the language documents
        // is how `uppercase("hi")` came to answer `Unknown function
        // 'uppercase'` for as long as it did.
        None if is_builtin(name) => Err(crate::error::Error::Runtime(
            format!("{name} was called with arguments it cannot use"),
            span,
        )),
        None => Ok(None),
    }
}

/// The text utilities that formatting, templates and diffs need.
///
/// They live in one dispatcher because they all fail by name: an unknown
/// placeholder, a width that is not a whole number and a fill of no characters
/// are all `RuntimeError`s a program can catch with `try`, not `Unknown
/// function` messages about a name the language has.
fn text_utility_builtin(
    span: crate::error::Span,
    name: &str,
    args: &[Value],
) -> crate::error::Result<Option<Value>> {
    let answer = match name {
        "text_wrap" => {
            let [Value::Text(text), width] = args else {
                return Err(text_utility_error(span, "wrap", "needs text and a width"));
            };
            let width = text_utility_width(span, "wrap", width)?;
            Value::list(
                wrap_text(text, width)
                    .into_iter()
                    .map(Value::Text)
                    .collect(),
            )
        }
        "text_diff" => {
            let [Value::Text(old), Value::Text(new)] = args else {
                return Err(text_utility_error(span, "diff", "needs two texts"));
            };
            Value::Text(unified_diff(span, old, new)?)
        }
        "text_dedent" => {
            let [Value::Text(text)] = args else {
                return Err(text_utility_error(span, "dedent", "needs one text"));
            };
            Value::Text(dedent_text(text))
        }
        "text_template" => {
            let [Value::Text(template), Value::Record(values)] = args else {
                return Err(text_utility_error(
                    span,
                    "template",
                    "needs a text and a record of values",
                ));
            };
            Value::Text(template_text(span, template, values)?)
        }
        "text_pad" => {
            let [Value::Text(text), width, Value::Text(fill)] = args else {
                return Err(text_utility_error(
                    span,
                    "pad",
                    "needs text, a width and a fill",
                ));
            };
            let width = text_utility_width(span, "pad", width)?;
            Value::Text(pad_text(span, text, width, fill)?)
        }
        "text_align" => {
            let [Value::Text(text), width, Value::Text(side)] = args else {
                return Err(text_utility_error(
                    span,
                    "align",
                    "needs text, a width and a side",
                ));
            };
            let width = text_utility_width(span, "align", width)?;
            Value::Text(align_text(span, text, width, side)?)
        }
        _ => return Ok(None),
    };
    Ok(Some(answer))
}

/// A named refusal from one of the text utilities.
fn text_utility_error(span: crate::error::Span, name: &str, what: &str) -> crate::error::Error {
    crate::error::Error::Runtime(format!("{name}: {what}"), span)
}

/// The largest width `text.wrap`, `text.pad` and `text.align` accept.
///
/// Padding and alignment allocate a character per column of the gap, so an
/// unbounded width would let a program ask the host for an arbitrary amount of
/// memory. A million characters is far beyond any formatting need and small
/// enough that the allocation is ordinary; a larger width is refused by name
/// instead of reaching an infallible allocation.
const MAX_TEXT_UTILITY_WIDTH: usize = 1_000_000;

/// The largest line table `text.diff` will build.
///
/// The LCS table costs one `usize` per pair of lines, so the bound is on
/// `(old + 1) * (new + 1)` rather than on either count alone. Four million
/// cells is about 32 MiB: enough for ordinary source files, and small enough
/// that an oversized pair is a catchable error rather than an aborted process.
const MAX_DIFF_CELLS: usize = 4_000_000;

/// The largest number of lines `text.diff` will read from either text.
///
/// The cell bound alone does not bound the line table's row vector: one line on
/// one side and a million on the other is a million rows, each a separate
/// allocation. Refusing a text past this many lines keeps that overhead
/// ordinary too, and the count is taken before the lines are materialised.
const MAX_DIFF_LINES: usize = 1_000_000;

/// Reads a width argument, refusing anything that is not a whole number between
/// one character and [`MAX_TEXT_UTILITY_WIDTH`].
fn text_utility_width(
    span: crate::error::Span,
    name: &str,
    value: &Value,
) -> crate::error::Result<usize> {
    match value {
        Value::Number(number)
            if number.is_finite()
                && number.fract() == 0.0
                && *number >= 1.0
                && *number <= MAX_TEXT_UTILITY_WIDTH as f64 =>
        {
            Ok(*number as usize)
        }
        Value::Number(_) => Err(text_utility_error(
            span,
            name,
            &format!("the width must be a whole number between 1 and {MAX_TEXT_UTILITY_WIDTH}"),
        )),
        _ => Err(text_utility_error(span, name, "the width must be a number")),
    }
}

/// Wraps `text` into lines no longer than `width` characters.
///
/// Words are moved whole to the next line. A single word longer than the width
/// has no line it can fit on, so it alone is broken into width-sized pieces;
/// every word that can fit somewhere is never split. Existing newlines start a
/// new output line and CRLF is read as a line ending rather than as content.
/// Horizontal whitespace is the separator between words: a run of spaces, a tab
/// or any other Unicode whitespace becomes the single space between two words,
/// so wrapping normalises it rather than preserving it.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut lines = Vec::new();

    for raw in body.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0usize;
        for word in line.split_whitespace() {
            let word_width = word.chars().count();
            if word_width > width {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    current_width = 0;
                }
                let characters: Vec<char> = word.chars().collect();
                for piece in characters.chunks(width) {
                    lines.push(piece.iter().collect());
                }
                continue;
            }

            if current.is_empty() {
                current.push_str(word);
                current_width = word_width;
            } else if current_width + 1 + word_width <= width {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_width;
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
                current_width = word_width;
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }

    lines
}

/// One line of a text, with whether it ended in a newline.
///
/// The flag is part of the line's identity: `a` and `a\n` are different lines
/// to a diff, which is why a text with no final newline carries the standard
/// `\ No newline at end of file` marker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffLine {
    text: String,
    has_newline: bool,
}

/// Splits `text` into the lines a unified diff counts.
fn diff_lines(text: &str) -> Vec<DiffLine> {
    text.split_inclusive('\n')
        .map(|chunk| match chunk.strip_suffix('\n') {
            Some(line) => DiffLine {
                text: line.to_string(),
                has_newline: true,
            },
            None => DiffLine {
                text: chunk.to_string(),
                has_newline: false,
            },
        })
        .collect()
}

/// Counts the lines `diff_lines` would produce, without allocating them.
fn diff_line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.bytes().filter(|byte| *byte == b'\n').count() + usize::from(!text.ends_with('\n'))
}

/// One step of the edit script: a line both texts share, or one only `old` or
/// only `new` has.
#[derive(Debug, Clone, Copy)]
enum DiffStep {
    Equal(usize),
    Delete(usize),
    Insert(usize),
}

/// A standard unified diff between `old` and `new`, or the empty text when they
/// are equal.
///
/// The shape is the one every unified-diff reader knows: `---`/`+++` headers,
/// `@@ -a,b +c,d @@` hunks with three context lines, and ` `/`+`/`-` prefixed
/// lines. A line with no final newline carries the standard marker.
fn unified_diff(span: crate::error::Span, old: &str, new: &str) -> crate::error::Result<String> {
    let old_len = diff_line_count(old);
    let new_len = diff_line_count(new);
    if old_len > MAX_DIFF_LINES || new_len > MAX_DIFF_LINES {
        return Err(text_utility_error(
            span,
            "diff",
            &format!(
                "the texts are too large to diff: {old_len} and {new_len} lines exceed the \
                 {MAX_DIFF_LINES}-line limit"
            ),
        ));
    }

    let cells = old_len
        .checked_add(1)
        .and_then(|old| new_len.checked_add(1).and_then(|new| old.checked_mul(new)))
        .ok_or_else(|| text_utility_error(span, "diff", "the texts are too large to diff"))?;
    if cells > MAX_DIFF_CELLS {
        return Err(text_utility_error(
            span,
            "diff",
            &format!(
                "the texts are too large to diff: {old_len} and {new_len} lines exceed the \
                 {MAX_DIFF_CELLS}-cell limit"
            ),
        ));
    }

    let old_lines = diff_lines(old);
    let new_lines = diff_lines(new);
    let mut lengths = vec![vec![0usize; new_len + 1]; old_len + 1];
    for i in (0..old_len).rev() {
        for j in (0..new_len).rev() {
            lengths[i][j] = if old_lines[i] == new_lines[j] {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }

    let mut steps = Vec::with_capacity(old_len + new_len);
    let (mut i, mut j) = (0usize, 0usize);
    while i < old_len && j < new_len {
        if old_lines[i] == new_lines[j] {
            steps.push(DiffStep::Equal(i));
            i += 1;
            j += 1;
        } else if lengths[i + 1][j] >= lengths[i][j + 1] {
            steps.push(DiffStep::Delete(i));
            i += 1;
        } else {
            steps.push(DiffStep::Insert(j));
            j += 1;
        }
    }
    while i < old_len {
        steps.push(DiffStep::Delete(i));
        i += 1;
    }
    while j < new_len {
        steps.push(DiffStep::Insert(j));
        j += 1;
    }

    if steps.iter().all(|step| matches!(step, DiffStep::Equal(_))) {
        return Ok(String::new());
    }

    let mut old_before = Vec::with_capacity(steps.len() + 1);
    let mut new_before = Vec::with_capacity(steps.len() + 1);
    let (mut old_seen, mut new_seen) = (0usize, 0usize);
    for step in &steps {
        old_before.push(old_seen);
        new_before.push(new_seen);
        match step {
            DiffStep::Equal(_) => {
                old_seen += 1;
                new_seen += 1;
            }
            DiffStep::Delete(_) => old_seen += 1,
            DiffStep::Insert(_) => new_seen += 1,
        }
    }
    old_before.push(old_seen);
    new_before.push(new_seen);

    let changes: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| match step {
            DiffStep::Equal(_) => None,
            DiffStep::Delete(_) | DiffStep::Insert(_) => Some(index),
        })
        .collect();

    let mut text = String::from("--- original\n+++ changed\n");
    let mut cursor = 0usize;
    while cursor < changes.len() {
        let start = changes[cursor].saturating_sub(3);
        let mut end = (changes[cursor] + 4).min(steps.len());
        cursor += 1;
        while cursor < changes.len() {
            let next_start = changes[cursor].saturating_sub(3);
            if next_start > end {
                break;
            }
            end = end.max((changes[cursor] + 4).min(steps.len()));
            cursor += 1;
        }

        let old_count = steps[start..end]
            .iter()
            .filter(|step| matches!(step, DiffStep::Equal(_) | DiffStep::Delete(_)))
            .count();
        let new_count = steps[start..end]
            .iter()
            .filter(|step| matches!(step, DiffStep::Equal(_) | DiffStep::Insert(_)))
            .count();
        let old_start = if old_count == 0 {
            old_before[start]
        } else {
            old_before[start] + 1
        };
        let new_start = if new_count == 0 {
            new_before[start]
        } else {
            new_before[start] + 1
        };

        text.push_str(&format!(
            "@@ -{} +{} @@\n",
            diff_range(old_start, old_count),
            diff_range(new_start, new_count)
        ));

        for step in &steps[start..end] {
            let (prefix, line) = match step {
                DiffStep::Equal(old_index) => (' ', &old_lines[*old_index]),
                DiffStep::Delete(old_index) => ('-', &old_lines[*old_index]),
                DiffStep::Insert(new_index) => ('+', &new_lines[*new_index]),
            };
            text.push(prefix);
            text.push_str(&line.text);
            text.push('\n');
            if !line.has_newline {
                text.push_str("\\ No newline at end of file\n");
            }
        }
    }

    Ok(text)
}

/// A unified-diff range: `start` alone when it covers one line, `start,count`
/// otherwise. A zero-length range starts at the line before it, as the format
/// requires.
fn diff_range(start: usize, count: usize) -> String {
    if count == 1 {
        start.to_string()
    } else {
        format!("{start},{count}")
    }
}

/// Removes the longest leading whitespace every non-blank line shares.
///
/// A blank line does not vote, so indentation around a blank line still
/// dedents. A non-blank line with no leading whitespace makes the common prefix
/// empty, which leaves every line exactly as it was rather than stripping some
/// and not others. Line endings are preserved, so CRLF input stays CRLF.
fn dedent_text(text: &str) -> String {
    let pieces: Vec<(&str, &str)> = text
        .split_inclusive('\n')
        .map(|piece| match piece.strip_suffix('\n') {
            Some(content) => match content.strip_suffix('\r') {
                Some(content) => (content, "\r\n"),
                None => (content, "\n"),
            },
            None => (piece, ""),
        })
        .collect();
    let mut prefix: Option<String> = None;
    for (content, _) in &pieces {
        if content.trim().is_empty() {
            continue;
        }
        let leading: String = content.chars().take_while(|c| c.is_whitespace()).collect();
        prefix = Some(match prefix {
            None => leading,
            Some(current) => common_prefix(&current, &leading),
        });
    }
    let prefix = prefix.unwrap_or_default();

    pieces
        .iter()
        .map(|&(content, ending)| {
            let body = content.strip_prefix(&prefix).unwrap_or(content);
            format!("{body}{ending}")
        })
        .collect()
}

/// The longest prefix two strings share.
fn common_prefix(left: &str, right: &str) -> String {
    left.chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a)
        .collect()
}

/// Substitutes `<<name>>` placeholders in `template` from `values`.
///
/// The delimiters are literal text rather than braces because Redblue strings
/// already use `{…}` for interpolation, so a template written as `{{name}}`
/// would be read as an interpolated expression before this function ever saw
/// it. `<<<<` writes the opening delimiter literally, which is the one escape
/// a template needs: `>>` outside a placeholder is already literal text.
fn template_text(
    span: crate::error::Span,
    template: &str,
    values: &Fields,
) -> crate::error::Result<String> {
    let mut result = String::new();
    let mut rest = template;

    while let Some(open) = rest.find("<<") {
        result.push_str(&rest[..open]);
        let after_open = &rest[open..];

        if let Some(stripped) = after_open.strip_prefix("<<<<") {
            result.push_str("<<");
            rest = stripped;
            continue;
        }

        let after = &after_open[2..];
        let Some(close) = after.find(">>") else {
            return Err(text_utility_error(
                span,
                "template",
                "a placeholder has no closing '>>'",
            ));
        };
        let name = after[..close].trim();
        if name.is_empty() {
            return Err(text_utility_error(
                span,
                "template",
                "a placeholder has no name",
            ));
        }
        let Some(value) = values.get(name) else {
            return Err(text_utility_error(
                span,
                "template",
                &format!("unknown placeholder '{name}'"),
            ));
        };
        result.push_str(&value.to_string());
        rest = &after[close + 2..];
    }

    result.push_str(rest);
    Ok(result)
}

/// Builds `left` copies of `fill`, then `text`, then `right` copies of `fill`.
///
/// The capacity is reserved with [`String::try_reserve_exact`] so a request the
/// host cannot satisfy is a named `RuntimeError` rather than an abort, and the
/// copies are written one character at a time so the loop is bounded by the
/// already-capped padding counts.
fn padded_text(
    span: crate::error::Span,
    name: &str,
    text: &str,
    left: usize,
    right: usize,
    fill: &str,
) -> crate::error::Result<String> {
    let widest_fill = fill.chars().map(char::len_utf8).max().unwrap_or(0);
    let bytes = left
        .checked_add(right)
        .and_then(|count| count.checked_mul(widest_fill))
        .and_then(|padding| padding.checked_add(text.len()))
        .ok_or_else(|| {
            text_utility_error(span, name, "the padded text is too large to allocate")
        })?;
    let mut result = String::new();
    result
        .try_reserve_exact(bytes)
        .map_err(|_| text_utility_error(span, name, "the padded text is too large to allocate"))?;
    for character in fill.chars().cycle().take(left) {
        result.push(character);
    }
    result.push_str(text);
    for character in fill.chars().cycle().take(right) {
        result.push(character);
    }
    Ok(result)
}

/// Right-pads `text` to `width` characters with repeated `fill`.
fn pad_text(
    span: crate::error::Span,
    text: &str,
    width: usize,
    fill: &str,
) -> crate::error::Result<String> {
    if fill.is_empty() {
        return Err(text_utility_error(
            span,
            "pad",
            "the fill must be at least one character",
        ));
    }
    let current = text.chars().count();
    let needed = width.saturating_sub(current);
    padded_text(span, "pad", text, 0, needed, fill)
}

/// Pads `text` to `width` characters on the named side.
///
/// The side is validated before the width short-circuit so a text already at or
/// past the width still refuses an unknown side rather than answering it
/// unchanged.
fn align_text(
    span: crate::error::Span,
    text: &str,
    width: usize,
    side: &str,
) -> crate::error::Result<String> {
    let current = text.chars().count();
    let needed = width.saturating_sub(current);
    match side {
        "left" => padded_text(span, "align", text, 0, needed, " "),
        "right" => padded_text(span, "align", text, needed, 0, " "),
        "center" => {
            let left = needed / 2;
            padded_text(span, "align", text, left, needed - left, " ")
        }
        _ => Err(text_utility_error(
            span,
            "align",
            "the side must be left, right or center",
        )),
    }
}

/// The `regex` module, bound to the crate rather than written in Redblue.
///
/// `regex_match` answers a match record or `nothing`; the record is ordinary
/// data (`text`, `start`, `end`, `groups`), so two matches with the same
/// captures compare equal. `groups[0]` is the whole match and `groups[n]` is
/// capture group `n`; a group that did not participate is `nothing`.
fn regex_builtin(
    span: crate::error::Span,
    name: &str,
    args: &[Value],
) -> crate::error::Result<Value> {
    match name {
        "regex_match" => {
            let (text, pattern) = regex_text_and_pattern(span, name, args)?;
            let expression = regex_compile(span, pattern)?;
            Ok(regex_match_value(text, &expression))
        }
        "regex_is_match" => {
            let (text, pattern) = regex_text_and_pattern(span, name, args)?;
            let expression = regex_compile(span, pattern)?;
            Ok(Value::YesNo(expression.is_match(text)))
        }
        "regex_capture" => {
            let (text, pattern, group) = match args {
                [Value::Text(text), Value::Text(pattern), Value::Number(group)] => {
                    (text, pattern, *group)
                }
                _ => {
                    return Err(regex_argument_error(
                        span,
                        name,
                        "text, a pattern and a group number",
                    ))
                }
            };
            if !group.is_finite() || group.fract() != 0.0 || group < 0.0 {
                return Err(crate::error::Error::Runtime(
                    format!("{name} requires a whole group number, but was given {group}"),
                    span,
                ));
            }
            let expression = regex_compile(span, pattern)?;
            let Some(captures) = expression.captures(text) else {
                return Ok(Value::Nothing);
            };
            if group >= expression.captures_len() as f64 {
                return Err(crate::error::Error::Runtime(
                    format!(
                        "{name}: pattern {pattern:?} has no capture group {group}; \
                         it has {}",
                        expression.captures_len().saturating_sub(1)
                    ),
                    span,
                ));
            }
            let group = group as usize;
            // A group that did not participate has no span, so `get` answers
            // `None`; that is `nothing`, not the empty text an expansion would
            // produce. An empty group that *did* participate still has a span
            // and answers `text("")`.
            match captures.get(group) {
                Some(matched) => Ok(Value::Text(matched.as_str().to_string())),
                None => Ok(Value::Nothing),
            }
        }
        "regex_replace" => {
            let (text, pattern, replacement) = match args {
                [Value::Text(text), Value::Text(pattern), Value::Text(replacement)] => {
                    (text, pattern, replacement)
                }
                _ => {
                    return Err(regex_argument_error(
                        span,
                        name,
                        "text, a pattern and a replacement",
                    ))
                }
            };
            let expression = regex_compile(span, pattern)?;
            regex_check_replacement(span, pattern, replacement, &expression)?;
            Ok(Value::Text(
                expression
                    .replace_all(text, replacement.as_str())
                    .into_owned(),
            ))
        }
        "regex_split" => {
            let (text, pattern) = regex_text_and_pattern(span, name, args)?;
            let expression = regex_compile(span, pattern)?;
            Ok(Value::list(
                expression
                    .split(text)
                    .map(|part| Value::Text(part.to_string()))
                    .collect(),
            ))
        }
        _ => Ok(Value::Nothing),
    }
}

fn regex_argument_error(
    span: crate::error::Span,
    name: &str,
    expected: &str,
) -> crate::error::Error {
    crate::error::Error::Runtime(format!("{name} requires {expected}"), span)
}

fn regex_text_and_pattern<'a>(
    span: crate::error::Span,
    name: &str,
    args: &'a [Value],
) -> crate::error::Result<(&'a str, &'a str)> {
    match args {
        [Value::Text(text), Value::Text(pattern)] => Ok((text, pattern)),
        _ => Err(regex_argument_error(span, name, "text and a pattern")),
    }
}

fn regex_compile(span: crate::error::Span, pattern: &str) -> crate::error::Result<regex::Regex> {
    regex::Regex::new(pattern).map_err(|failure| {
        crate::error::Error::Runtime(
            format!("regex pattern {pattern:?} is not valid: {failure}"),
            span,
        )
    })
}

fn regex_match_value(text: &str, expression: &regex::Regex) -> Value {
    let Some(captures) = expression.captures(text) else {
        return Value::Nothing;
    };
    let Some(whole) = captures.get(0) else {
        return Value::Nothing;
    };
    let mut fields = Fields::new();
    fields.insert("text".to_string(), Value::Text(whole.as_str().to_string()));
    fields.insert(
        "start".to_string(),
        Value::Number(text[..whole.start()].chars().count() as f64),
    );
    fields.insert(
        "finish".to_string(),
        Value::Number(text[..whole.end()].chars().count() as f64),
    );
    fields.insert(
        "groups".to_string(),
        Value::list(
            captures
                .iter()
                .map(|matched| match matched {
                    Some(matched) => Value::Text(matched.as_str().to_string()),
                    None => Value::Nothing,
                })
                .collect(),
        ),
    );
    Value::record(fields)
}

/// Refuses a replacement that names a capture group the pattern does not have.
///
/// `regex`'s own `expand` treats an unknown group as an empty string, which
/// turns a typo in `$2` into silently deleted text. A group reference the
/// pattern cannot satisfy is a runtime failure with the pattern and the name in
/// it, so `try … catch error of Runtime` can report it.
fn regex_check_replacement(
    span: crate::error::Span,
    pattern: &str,
    replacement: &str,
    expression: &regex::Regex,
) -> crate::error::Result<()> {
    let bytes = replacement.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        match bytes.get(index + 1) {
            Some(b'$') => {
                index += 2;
                continue;
            }
            Some(b'{') => {
                let Some(close) = replacement[index + 2..].find('}') else {
                    return Err(crate::error::Error::Runtime(
                        format!(
                            "regex.replace: replacement {replacement:?} has an unclosed \
                             capture reference"
                        ),
                        span,
                    ));
                };
                let end = index + 2 + close;
                let name = &replacement[index + 2..end];
                regex_check_group(span, pattern, name, expression)?;
                index = end + 1;
                continue;
            }
            Some(next) if next.is_ascii_alphanumeric() || *next == b'_' => {
                let start = index + 1;
                let mut end = start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                let name = &replacement[start..end];
                regex_check_group(span, pattern, name, expression)?;
                index = end;
                continue;
            }
            _ => {
                index += 1;
            }
        }
    }
    Ok(())
}

fn regex_check_group(
    span: crate::error::Span,
    pattern: &str,
    name: &str,
    expression: &regex::Regex,
) -> crate::error::Result<()> {
    let known = if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit()) {
        name.parse::<usize>()
            .is_ok_and(|group| group < expression.captures_len())
    } else {
        expression
            .capture_names()
            .flatten()
            .any(|capture| capture == name)
    };
    if known {
        Ok(())
    } else {
        Err(crate::error::Error::Runtime(
            format!("regex.replace: pattern {pattern:?} has no capture group named {name:?}"),
            span,
        ))
    }
}

/// Whether `name` is a builtin this crate can answer, used to tell a missing
/// builtin from a missing user function.
pub fn is_builtin(name: &str) -> bool {
    BUILTIN_NAMES.contains(&name)
}

/// Whether `name` is an internal builtin whose only spelling is as a member of a
/// module.
///
/// The phase-101 text utilities, `sys.parse` and the phase-103 logging calls
/// are stored under `<module>_<member>` because that is the name a dotted call
/// resolves to, but they are not part of the flat builtin surface: `text.wrap`,
/// `sys.parse` and `logging.debug` answer and a bare `text_wrap`, `sys_parse`
/// or `logging_debug` does not. They are deliberately absent from
/// [`BUILTIN_NAMES`] and [`builtins`], so `is_builtin` does not advertise them
/// and the analyzer, REPL completer, highlighter and introspection never offer
/// a name a program cannot call. A program that wants a flat helper keeps
/// writing `wrap`, or declares the exact internal spelling itself, because
/// these remain ordinary names a program may declare. Both engines refuse the
/// bare spelling unless the program has declared that name itself, so this
/// cannot depend on which one ran the program.
pub fn is_module_only_builtin(name: &str) -> bool {
    matches!(
        name,
        "text_wrap"
            | "text_diff"
            | "text_dedent"
            | "text_template"
            | "text_pad"
            | "text_align"
            | "sys_parse"
            | "logging_debug"
            | "logging_info"
            | "logging_warn"
            | "logging_error"
            | "logging_set_level"
            | "logging_level"
            | "logging_set_timestamps"
            | "logging_set_destination"
            | "logging_reset"
    )
}

/// Resolves `module.member` to the builtin name behind it.
///
/// The internal encoding of a module function is `module_member` — that is what
/// `qualified_member` builds and what the global table registers, which is why
/// `json.parse` is stored as `json_parse`. The *documented* modules are spelled
/// differently from the ones this table was grown from: `text.uppercase` is
/// stored as nothing, so the dotted spelling reached the VM and found nothing.
///
/// This only unwraps a name the table does **not** already hold. That
/// qualification is load-bearing: `files.append` is registered as
/// `files_append`, and unwrapping it to the bare `append` sent a working
/// two-argument call to a builtin that needs a binding to write through, so
/// `files.append(path, "a")` was refused as arguments it cannot use. A name
/// the table holds is already the right name.
///
/// Beyond that the unwrapping is **enumerated**: `<module>_<member>` unwraps to
/// `member` only when [`module_has_member`] says `<module>.<member>` is a
/// spelling the language has. Matching any module against any builtin instead
/// made every module a namespace for every flat builtin — `console.input`,
/// `math.random` and `json.uppercase` all answered, which is the opposite of
/// what SPEC.md says about each of them (`there is no console.input`, `math.random
/// is not implemented`, and a module has the functions spelled `<module>_<member>`
/// in the registry). A module is a surface somebody agreed to, so the surface is
/// a list and this is where the list is enforced.
///
/// `None` means the name is not an unwrappable module member, so the caller
/// carries on with the name it was given and — for a receiver that is a module —
/// refuses the call by its dotted name.
pub fn module_member_name(qualified: &str) -> Option<&str> {
    // The module-only builtins are not in `BUILTIN_NAMES`, so their dotted
    // spelling has no registry entry to find. The member table says the pair
    // exists, and the dispatcher answers the internal `<module>_<member>` name
    // itself; returning the name unchanged is what makes `text.wrap` and
    // `sys.parse` reach it.
    if is_module_only_builtin(qualified) {
        let (module, member) = qualified.split_once('_')?;
        return module_has_member(module, member).then_some(qualified);
    }
    if BUILTIN_NAMES.contains(&qualified) {
        return None;
    }
    let (module, member) = qualified.split_once('_')?;
    // The bare name has to be a registered builtin as well. Enumerating the
    // members says which spellings exist; it does not make them answer, and a
    // member whose builtin is gone has to be refused by name rather than reach a
    // dispatcher that will report a function nobody registered.
    if module_has_member(module, member) && BUILTIN_NAMES.contains(&member) {
        Some(member)
    } else {
        None
    }
}

/// The one name a builtin is answerable under, given the name the VM holds.
///
/// A module member resolves to its bare builtin; anything else is itself. Both
/// engines go through this, which is what keeps `text.uppercase` and
/// `uppercase` from being two different functions.
pub fn resolve(qualified: &str) -> &str {
    module_member_name(qualified).unwrap_or(qualified)
}

/// The internal name a compiled `lru_cache(n) to ...` calls after binding the
/// function.
///
/// It is not part of the language surface: the tree-walker records the cache
/// when it executes the declaration, and the bytecode compiler emits this call
/// so both engines end up with the same registry entry. The leading NUL is a
/// character the lexer cannot put in an identifier, so no program can spell the
/// name and intercept the declaration — a legal-looking name such as
/// `__lru_cache_declare` would be callable and would mean different things to
/// the two engines. It is not in [`BUILTIN_NAMES`].
pub const LRU_CACHE_DECLARE: &str = "\u{0}lru_cache_declare";

/// The internal name a compiled `lru_cache(n) to can ...` calls after the
/// object that owns the method is registered.
///
/// A method is stored on its object type rather than bound to a name, so it
/// needs a three-argument declaration — object, method and capacity — and the
/// call is emitted after `DEF_OBJECT`/`STORE` has registered the type. Like
/// [`LRU_CACHE_DECLARE`] the leading NUL keeps any program from spelling it.
pub const LRU_CACHE_DECLARE_METHOD: &str = "\u{0}lru_cache_declare_method";

/// Every name [`builtins`] registers.
///
/// This is the list a registered-but-unreachable builtin shows up against, and
/// `tests/stdlib_module_docs_test.rs` walks the registry itself rather than
/// trusting this constant to stay in step.
pub const BUILTIN_NAMES: &[&str] = &[
    "abs",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "pow",
    "sin",
    "cos",
    "tan",
    "log",
    "exp",
    "mean",
    "median",
    "mode",
    "variance",
    "standard_deviation",
    "percentile",
    "correlation",
    "uppercase",
    "lowercase",
    "trim",
    "split",
    "join",
    "contains",
    "starts_with",
    "ends_with",
    "replace",
    "length",
    "push",
    "append",
    "pop",
    "shift",
    "map",
    "filter",
    "reduce",
    "chain",
    "zip",
    "enumerate",
    "product",
    "groupby",
    "take",
    "skip",
    "list_map",
    "collections_counter",
    "collections_count",
    "collections_most_common",
    "collections_defaultdict",
    "collections_get",
    "collections_put",
    "collections_deque",
    "collections_push_left",
    "collections_push_right",
    "collections_pop_left",
    "collections_pop_right",
    "collections_namedtuple",
    "collections_field",
    "is_number",
    "is_text",
    "is_list",
    "is_record",
    "to_text",
    "to_number",
    "to_list",
    "type_of",
    "decimal_from_text",
    "decimal_from_number",
    "decimal_to_text",
    "decimal_to_number",
    "decimal_sum",
    "files_read",
    "files_write",
    "files_append",
    "files_exists",
    "files_lines",
    "files_delete",
    "files_copy",
    "files_rename",
    "path_join",
    "path_parent",
    "path_filename",
    "path_stem",
    "path_extension",
    "path_is_absolute",
    "path_resolve",
    "path_normalize",
    "path_list",
    "time_now",
    "time_sleep",
    "time_format",
    "time_parse",
    "time_unix",
    "time_from_unix",
    "time_date",
    "time_datetime",
    "time_duration",
    "time_add_years",
    "time_add_months",
    "time_add_days",
    "time_add_hours",
    "time_add_minutes",
    "time_add_seconds",
    "json_parse",
    "json_stringify",
    "csv_parse",
    "csv_stringify",
    "json_parse_lines",
    "json_stringify_lines",
    "toml_parse",
    "tar_read",
    "tar_write",
    "zip_read",
    "zip_write",
    "matrix_zeros",
    "matrix_ones",
    "matrix_identity",
    "matrix_from_lists",
    "matrix_to_lists",
    "matrix_shape",
    "matrix_add",
    "matrix_subtract",
    "matrix_multiply",
    "matrix_divide",
    "matrix_scale",
    "matrix_matmul",
    "matrix_transpose",
    "matrix_sum",
    "matrix_mean",
    "matrix_min",
    "matrix_max",
    "matrix_at",
    "matrix_row",
    "matrix_column",
    "network_get",
    "network_post",
    "regex_match",
    "regex_is_match",
    "regex_capture",
    "regex_replace",
    "regex_split",
    "expect",
    "assert",
    "console_log",
    "console_error",
    "console_clear",
    "random",
    "random_seed",
    "random_number",
    "random_choice",
    "random_shuffle",
    "bytes_from_text",
    "bytes_write",
    "bytes_text",
    "base64_encode",
    "base64_decode",
    "hex_encode",
    "hex_decode",
    "hash_sha256",
    "hash_sha512",
    "uuid_generate",
    "uuid_parse",
    "sys_argv",
    "input",
];

/// The shape of every builtin: the parameters it reads and what it answers.
///
/// This is what `:sighelp` reads. A builtin's arity and argument types are written
/// in the arms of [`builtin_function`] and [`crate::runtime::builtin`], which a
/// reader has to hold in their head to answer "how do I call `split`?" — and a
/// REPL that cannot answer it sends the user to the source instead.
///
/// Kept here, beside [`BUILTIN_NAMES`], so it is written down where the registry
/// is: [`signatures_cover_the_registry`] fails the moment a builtin is registered
/// without a shape, which is the drift a table in a REPL module would have had.
///
/// The shapes are written in the language's own terms — `text`, `number`,
/// `yes/no`, `list` — rather than Rust's, because they are shown to someone
/// writing Redblue. `text.length` answers bytes, and says so.
pub const BUILTIN_SIGNATURES: &[(&str, &str)] = &[
    (
        "abs",
        "abs(number or decimal) -> the same kind, without its sign",
    ),
    (
        "floor",
        "floor(number or decimal) -> number or decimal, rounded down",
    ),
    (
        "ceil",
        "ceil(number or decimal) -> number or decimal, rounded up",
    ),
    (
        "round",
        "round(number or decimal) -> number or decimal, nearest, ties away from zero",
    ),
    ("sqrt", "sqrt(number) -> number, or nothing for a negative"),
    (
        "pow",
        "pow(number, number) -> number, or nothing when it overflows",
    ),
    ("sin", "sin(number) -> number"),
    ("cos", "cos(number) -> number"),
    ("tan", "tan(number) -> number, or nothing at a pole"),
    ("log", "log(number) -> number, or nothing for zero or less"),
    ("exp", "exp(number) -> number, or nothing when it overflows"),
    ("mean", "mean(list of number) -> number; the list must not be empty"),
    ("median", "median(list of number) -> number; an even list averages the two middle values"),
    ("mode", "mode(list of number) -> list of number; every tied value, ascending"),
    (
        "variance",
        "variance(list of number) -> number; the population variance",
    ),
    (
        "standard_deviation",
        "standard_deviation(list of number) -> number; the population square root of the variance",
    ),
    (
        "percentile",
        "percentile(list of number, number) -> number; rank 0 to 100, linearly interpolated",
    ),
    (
        "correlation",
        "correlation(list of number, list of number) -> number from -1 to 1",
    ),
    (
        "uppercase",
        "uppercase(text) -> text, with each character's uppercase form",
    ),
    (
        "lowercase",
        "lowercase(text) -> text, with each character's lowercase form",
    ),
    (
        "trim",
        "trim(text) -> text, without its leading and trailing space",
    ),
    (
        "split",
        "split(text, separator) -> list of text; an empty separator has no answer",
    ),
    ("join", "join(list, separator) -> text"),
    (
        "contains",
        "contains(text, text) -> yes/no, searching for the text\n\
         contains(list, any) -> yes/no, testing membership",
    ),
    ("starts_with", "starts_with(text, prefix) -> yes/no"),
    ("ends_with", "ends_with(text, suffix) -> yes/no"),
    (
        "replace",
        "replace(text, from, to) -> text; an empty `from` has no answer",
    ),
    (
        "text_wrap",
        "wrap(text, number) -> a list of lines no longer than the width, breaking a word only when it alone is too long",
    ),
    (
        "text_diff",
        "diff(old text, new text) -> a standard unified diff, empty when the texts are equal",
    ),
    (
        "text_dedent",
        "dedent(text) -> text with the common leading whitespace of its non-blank lines removed",
    ),
    (
        "text_template",
        "template(text, record) -> text with <<name>> placeholders replaced; <<<< writes a literal <<",
    ),
    (
        "text_pad",
        "pad(text, number, text) -> text right-padded to the width with the fill",
    ),
    (
        "text_align",
        "align(text, number, text) -> text padded to the width on the named side: left, right or center",
    ),
    (
        "length",
        "length(text) -> number of bytes\nlength(list) -> number of items",
    ),
    (
        "push",
        "push(list, value) -> the list with the value at its end",
    ),
    (
        "append",
        "append(value, list) -> the list with the value at its start",
    ),
    (
        "pop",
        "pop(list) -> the last item, or nothing for an empty list",
    ),
    (
        "shift",
        "shift(list) -> the first item, or nothing for an empty list",
    ),
    (
        "map",
        "map(list, function) -> a list of what the function answers for each item",
    ),
    (
        "filter",
        "filter(list, predicate) -> the items the predicate accepts",
    ),
    ("reduce", "reduce(list, function, start) -> one value"),
    (
        "chain",
        "chain(list, list, ...) -> a list of every argument list's items in the order given",
    ),
    (
        "zip",
        "zip(list, list, ...) -> a list of rows, one item from each list, stopping at the shortest",
    ),
    (
        "enumerate",
        "enumerate(list) -> a list of [index, item] pairs, indexes from zero",
    ),
    (
        "product",
        "product(list, list) -> a list of [a, b] pairs, one for each combination",
    ),
    (
        "groupby",
        "groupby(list) -> a list of {key, items} records in first-appearance order",
    ),
    (
        "take",
        "take(list, number) -> the first n items (a whole number), or the whole list when n is past it",
    ),
    (
        "skip",
        "skip(list, number) -> the list without its first n items (a whole number)",
    ),
    (
        "collections_counter",
        "collections.counter(list) -> {counts: [[key, count], ...], missing: 0}",
    ),
    (
        "collections_count",
        "collections.count(counter, key) -> the count, or the counter's missing value",
    ),
    (
        "collections_most_common",
        "collections.most_common(counter) -> [[key, count], ...], most common first",
    ),
    (
        "collections_defaultdict",
        "collections.defaultdict(factory) -> {entries: [], factory}",
    ),
    (
        "collections_get",
        "collections.get(dict, key) -> {value, dict}; a missing key is added with the factory value",
    ),
    (
        "collections_put",
        "collections.put(dict, key, value) -> the dict with the key set",
    ),
    (
        "collections_deque",
        "collections.deque() -> an empty deque\ncollections.deque(list) -> a deque of the list's items",
    ),
    (
        "collections_push_left",
        "collections.push_left(deque, value) -> the deque with the value at its front",
    ),
    (
        "collections_push_right",
        "collections.push_right(deque, value) -> the deque with the value at its back",
    ),
    (
        "collections_pop_left",
        "collections.pop_left(deque) -> {value, deque}; nothing at the front of an empty deque",
    ),
    (
        "collections_pop_right",
        "collections.pop_right(deque) -> {value, deque}; nothing at the back of an empty deque",
    ),
    (
        "collections_namedtuple",
        "collections.namedtuple(list of names, list of values) -> a record with those fields",
    ),
    (
        "collections_field",
        "collections.field(named record, text) -> the field's value; a missing field is refused by name",
    ),
    (
        "list_map",
        "list_map(list, function) -> a list; the `list.map` spelling",
    ),
    ("is_number", "is_number(any) -> yes/no"),
    ("is_text", "is_text(any) -> yes/no"),
    ("is_list", "is_list(any) -> yes/no"),
    ("is_record", "is_record(any) -> yes/no"),
    ("to_text", "to_text(any) -> text"),
    (
        "to_number",
        "to_number(text) -> number, or nothing when it is not one",
    ),
    (
        "to_list",
        "to_list(list) -> the list; to_list(text) -> its whitespace-separated words",
    ),
    ("type_of", "type_of(any) -> the name of its kind"),
    (
        "decimal_from_text",
        "decimal.from_text(text) -> decimal, exactly as written",
    ),
    (
        "decimal_from_number",
        "decimal.from_number(number) -> decimal, the number's shortest round-tripping text",
    ),
    ("decimal_to_text", "decimal.to_text(decimal) -> text"),
    (
        "decimal_to_number",
        "decimal.to_number(decimal) -> number, which is lossy when the decimal is finer than an f64",
    ),
    (
        "decimal_sum",
        "decimal.sum(list of decimal) -> decimal, exactly; the empty list is zero",
    ),
    ("files_read", "files_read(text) -> the whole file as text"),
    (
        "files_write",
        "files_write(text, text) -> nothing; writes a file",
    ),
    (
        "files_append",
        "files_append(text, text) -> nothing; adds to a file",
    ),
    ("files_exists", "files_exists(text) -> yes/no"),
    (
        "files_lines",
        "files_lines(text) -> a list of the file's lines",
    ),
    (
        "files_delete",
        "files_delete(text) -> nothing; removes a file",
    ),
    (
        "files_copy",
        "files_copy(text, text) -> nothing; copies a file",
    ),
    (
        "files_rename",
        "files_rename(text, text) -> nothing; moves a file",
    ),
    (
        "path_join",
        "path_join(text, ...) -> a path joined with the platform's separator",
    ),
    (
        "path_parent",
        "path_parent(text) -> the directory holding it, or nothing for a root",
    ),
    ("path_filename", "path_filename(text) -> the last name, or nothing"),
    ("path_stem", "path_stem(text) -> the name without its extension"),
    ("path_extension", "path_extension(text) -> the extension without its dot"),
    (
        "path_is_absolute",
        "path_is_absolute(text) -> yes/no, whether the host calls it absolute",
    ),
    (
        "path_resolve",
        "path_resolve(text) -> an absolute path with . and .. removed",
    ),
    (
        "path_normalize",
        "path_normalize(text) -> the path cleaned of . and .. without making it absolute",
    ),
    (
        "path_list",
        "path_list(text) -> the names in a directory, sorted",
    ),
    ("time_now", "time_now() -> a record of the current time"),
    (
        "time_sleep",
        "time_sleep(number) -> nothing; waits that many seconds",
    ),
    (
        "time_format",
        "time_format(date or number, text) -> text; the time in a format",
    ),
    (
        "time_parse",
        "time_parse(text, text) -> date; an ISO-8601 timestamp, or text in a chosen layout, as a date",
    ),
    (
        "time_unix",
        "time_unix(text or date) -> number; the instant as seconds since 1970",
    ),
    (
        "time_from_unix",
        "time_from_unix(number) -> date; the instant that many seconds after 1970-01-01T00:00:00Z",
    ),
    (
        "time_date",
        "time_date(number, number, number) -> a date at midnight",
    ),
    (
        "time_datetime",
        "time_datetime(number, number, number, number, number, number) -> a date and time",
    ),
    (
        "time_duration",
        "time_duration(number) -> a duration of that many seconds",
    ),
    (
        "time_add_years",
        "time_add_years(date, number) -> the date that many years later",
    ),
    (
        "time_add_months",
        "time_add_months(date, number) -> the date that many months later",
    ),
    (
        "time_add_days",
        "time_add_days(date, number) -> the date that many days later",
    ),
    (
        "time_add_hours",
        "time_add_hours(date, number) -> the date that many hours later",
    ),
    (
        "time_add_minutes",
        "time_add_minutes(date, number) -> the date that many minutes later",
    ),
    (
        "time_add_seconds",
        "time_add_seconds(date, number) -> the date that many seconds later",
    ),
    (
        "json_parse",
        "json_parse(text) -> the record, list or value it holds",
    ),
    ("json_stringify", "json_stringify(any) -> text"),
    (
        "csv_parse",
        "csv_parse(text) -> a list of rows, each a list of text",
    ),
    (
        "csv_stringify",
        "csv_stringify(rows) -> text; quotes commas, quotes, newlines and surrounding space",
    ),
    (
        "json_parse_lines",
        "json_parse_lines(text) -> a list of the JSON values, one per line",
    ),
    (
        "json_stringify_lines",
        "json_stringify_lines(list) -> text; one JSON value per line",
    ),
    ("toml_parse", "toml_parse(text) -> a record of the TOML document"),
    ("zip_read", "zip_read(path) -> a record of entry names to contents"),
    (
        "zip_write",
        "zip_write(path, entries) -> nothing; a stored (uncompressed) zip archive",
    ),
    ("tar_read", "tar_read(path) -> a record of entry names to contents"),
    (
        "tar_write",
        "tar_write(path, entries) -> nothing; a tar archive of the entries",
    ),
    (
        "matrix_zeros",
        "matrix.zeros(rows, columns) -> a matrix of zeros",
    ),
    (
        "matrix_ones",
        "matrix.ones(rows, columns) -> a matrix of ones",
    ),
    (
        "matrix_identity",
        "matrix.identity(size) -> a square identity matrix",
    ),
    (
        "matrix_from_lists",
        "matrix.from_lists(rows) -> a rectangular matrix; a ragged list is refused",
    ),
    (
        "matrix_to_lists",
        "matrix.to_lists(matrix) -> the rows as a list of lists",
    ),
    (
        "matrix_shape",
        "matrix.shape(matrix) -> [rows, columns]",
    ),
    (
        "matrix_add",
        "matrix.add(left, right) -> elementwise sum; shapes must match exactly",
    ),
    (
        "matrix_subtract",
        "matrix.subtract(left, right) -> elementwise difference; shapes must match exactly",
    ),
    (
        "matrix_multiply",
        "matrix.multiply(left, right) -> elementwise product; shapes must match exactly",
    ),
    (
        "matrix_divide",
        "matrix.divide(left, right) -> elementwise quotient; shapes must match exactly, and a zero divisor is refused",
    ),
    (
        "matrix_scale",
        "matrix.scale(matrix, number) -> every element multiplied by the number",
    ),
    (
        "matrix_matmul",
        "matrix.matmul(left, right) -> matrix product; the inner dimensions must match",
    ),
    (
        "matrix_transpose",
        "matrix.transpose(matrix) -> rows and columns exchanged",
    ),
    (
        "matrix_sum",
        "matrix.sum(matrix) -> the sum of every element; an empty matrix sums to 0",
    ),
    (
        "matrix_mean",
        "matrix.mean(matrix) -> the arithmetic mean; an empty matrix is refused",
    ),
    (
        "matrix_min",
        "matrix.min(matrix) -> the smallest element; an empty matrix is refused",
    ),
    (
        "matrix_max",
        "matrix.max(matrix) -> the largest element; an empty matrix is refused",
    ),
    (
        "matrix_at",
        "matrix.at(matrix, row, column) -> one element; a bad index is refused",
    ),
    (
        "matrix_row",
        "matrix.row(matrix, row) -> that row as a list",
    ),
    (
        "matrix_column",
        "matrix.column(matrix, column) -> that column as a list",
    ),
    (
        "network_get",
        "network_get(text) -> the body of the response",
    ),
    (
        "network_post",
        "network_post(text, text) -> the body of the response",
    ),
    (
        "regex_match",
        "regex_match(text, pattern) -> a match record, or nothing when there is no match",
    ),
    (
        "regex_is_match",
        "regex_is_match(text, pattern) -> yes/no, whether the pattern matches anywhere",
    ),
    (
        "regex_capture",
        "regex_capture(text, pattern, group) -> the captured text, or nothing",
    ),
    (
        "regex_replace",
        "regex_replace(text, pattern, replacement) -> text with $1 or ${name} captured",
    ),
    (
        "regex_split",
        "regex_split(text, pattern) -> a list of the text between matches",
    ),
    (
        "expect",
        "expect(actual, expected) -> nothing, or an error when they differ",
    ),
    (
        "assert",
        "assert(actual, expected) -> nothing, or an error when they differ",
    ),
    ("console_log", "console_log(any) -> nothing; writes a line"),
    (
        "console_error",
        "console_error(any) -> nothing; writes a line to stderr",
    ),
    (
        "console_clear",
        "console_clear() -> nothing; clears the screen",
    ),
    (
        "logging_debug",
        "logging.debug(text) -> nothing; writes a debug record at or above the threshold",
    ),
    (
        "logging_info",
        "logging.info(text) -> nothing; writes an info record at or above the threshold",
    ),
    (
        "logging_warn",
        "logging.warn(text) -> nothing; writes a warn record at or above the threshold",
    ),
    (
        "logging_error",
        "logging.error(text) -> nothing; writes an error record at or above the threshold",
    ),
    (
        "logging_set_level",
        "logging.set_level(text) -> nothing; sets the threshold to debug, info, warn, error or off",
    ),
    (
        "logging_level",
        "logging.level() -> text; the current threshold",
    ),
    (
        "logging_set_timestamps",
        "logging.set_timestamps(yes/no) -> nothing; whether records carry a timestamp",
    ),
    (
        "logging_set_destination",
        "logging.set_destination(text) -> nothing; stderr, stdout or a file path",
    ),
    (
        "logging_reset",
        "logging.reset() -> nothing; restores the default threshold, destination and timestamps",
    ),
    (
        "random",
        "random() -> a whole number from 0 to 99\nrandom(min, max) -> one between them",
    ),
    (
        "random_seed",
        "random_seed(number) -> nothing; seeds the generator",
    ),
    (
        "random_number",
        "random_number() -> a number from 0 to 1, exclusive",
    ),
    ("random_choice", "random_choice(list) -> one of its items"),
    (
        "random_shuffle",
        "random_shuffle(list) -> the items in a new order",
    ),
    (
        "bytes_from_text",
        "bytes_from_text(text) -> the text's bytes",
    ),
    (
        "bytes_write",
        "bytes_write(path, bytes) -> write the byte values to a file",
    ),
    ("bytes_text", "bytes_text(bytes) -> the bytes as text"),
    (
        "base64_encode",
        "base64.encode(bytes) -> base64 text with canonical padding",
    ),
    (
        "base64_decode",
        "base64.decode(text) -> a byte list; invalid input raises an error naming the position",
    ),
    (
        "hex_encode",
        "hex.encode(bytes) -> lowercase hexadecimal text",
    ),
    (
        "hex_decode",
        "hex.decode(text) -> a byte list; invalid input raises an error naming the position",
    ),
    (
        "hash_sha256",
        "hash.sha256(text or bytes) -> a lowercase SHA-256 digest",
    ),
    (
        "hash_sha512",
        "hash.sha512(text or bytes) -> a lowercase SHA-512 digest",
    ),
    (
        "uuid_generate",
        "uuid.generate() -> a new random UUID (version 4), canonical lowercase",
    ),
    (
        "uuid_parse",
        "uuid.parse(text) -> the canonical lowercase UUID; invalid input raises an error",
    ),
    (
        "sys_argv",
        "sys_argv() -> the arguments the program was run with",
    ),
    (
        "sys_parse",
        "sys_parse(declaration) -> a record of typed flags, arguments, help and usage\nsys_parse(declaration, arguments) -> the same, reading the given argument list",
    ),
    ("input", "input() -> the next line typed, as text"),
];

/// The shape of `name`, or `None` when it is not a builtin this table covers.
///
/// A `<module>_<member>` name is asked about as the member is spelled:
/// `:sighelp files.read` finds `files_read` because that is the name the
/// registry holds it under.
pub fn builtin_signature(name: &str) -> Option<&'static str> {
    let name = resolve(name);
    BUILTIN_SIGNATURES
        .iter()
        .find(|(listed, _)| *listed == name)
        .map(|(_, shape)| *shape)
}

/// The registered builtins [`BUILTIN_SIGNATURES`] says nothing about.
///
/// Should always be empty, and is checked rather than trusted: a shape written
/// for a builtin that no longer exists, and a builtin added without one, are the
/// same defect seen from the two ends — a `:sighelp` that answers about a name
/// the language does not have, or stays silent about one it does. Module-only
/// names are not in [`BUILTIN_NAMES`] but do have shapes, because the dotted
/// spelling reaches them, so they are allowed here.
pub fn signatures_covering_nothing() -> Vec<&'static str> {
    BUILTIN_SIGNATURES
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !BUILTIN_NAMES.contains(name) && !is_module_only_builtin(name))
        .collect()
}

/// The registered builtins with no shape written down.
///
/// Should always be empty, for the reason [`signatures_covering_nothing`] gives
/// from the other side.
pub fn builtins_without_signatures() -> Vec<&'static str> {
    BUILTIN_NAMES
        .iter()
        .copied()
        .filter(|name| builtin_signature(name).is_none())
        .collect()
}

/// The builtins whose effects leave the value they answer.
///
/// Everything else in the registry answers from its arguments and changes
/// nothing: `uppercase("a")` is `A` whether it is called twice or never, and
/// `length(xs)` is a number either way. These are not that.
///
/// **Three kinds, and a question cannot undo any of them.**
///
/// - They *write*: a file (`files_write`, `files_append`, `files_delete`,
///   `files_copy`, `files_rename`), the terminal (`console_log`,
///   `console_error`, `console_clear`), or a request out
///   (`network_get`, `network_post`). What they wrote is gone.
/// - They *block*: `time_sleep` waits, and `input` reads the session's own
///   standard input — which on a piped session is the rest of the script, and
///   on a terminal is the user pressing enter for a question they did not ask.
///   `files_read` and `files_lines` block the same way on a FIFO or a device,
///   with no timeout to save them.
/// - They *advance something shared*: `random`, `random_number`,
///   `random_choice` and `random_shuffle` all draw from the one
///   `RANDOM_STATE` ([`crate::runtime`]), so asking `:type random()` twice
///   answers two different numbers and the *next* `random()` a program draws
///   is not the one it would have got. `random_seed` writes that state outright.
///   `time_now` is the same defect without a stream to advance: it is the wall
///   clock, so the answer is different every time it is asked and there is no
///   answer to reproduce.
///
/// The first kind is what this table was written for, and it was too narrow in
/// the other two directions twice over: a read was treated as harmless because
/// it changes nothing on disk (it can block, and it reads what the user has not
/// agreed to show), and a draw was treated as harmless because it returns a
/// value (it moves a generator the next answer depends on).
///
/// The table exists because [`crate::repl::introspect::expression_of`] has to
/// refuse a call before it runs rather than after. A *program's* own body cannot
/// be read for effects before it is run in general — which is why the REPL also
/// refuses at the point of the call — but the registry can be, and it is here.
///
/// Written under the registry's own spelling (`files_write`, not `files.write`)
/// because that is what [`resolve`] is handed and what the registry holds;
/// [`is_effectful`] resolves first, so both spellings of a module member are the
/// same answer.
///
/// What it is not: a claim that these are the *only* ways out of the value
/// system. A `to` function a session wrote is not here, because nothing can be
/// known about its body before it runs.
pub const EFFECTFUL_BUILTINS: &[&str] = &[
    "files_read",
    "files_write",
    "files_append",
    "files_exists",
    "files_lines",
    "files_delete",
    "files_copy",
    "files_rename",
    "path_list",
    "zip_read",
    "zip_write",
    "tar_read",
    "tar_write",
    "network_get",
    "network_post",
    "time_now",
    "time_sleep",
    "input",
    "console_log",
    "console_error",
    "console_clear",
    "logging_debug",
    "logging_info",
    "logging_warn",
    "logging_error",
    "logging_set_level",
    "logging_set_timestamps",
    "logging_set_destination",
    "logging_reset",
    "random",
    "random_seed",
    "random_number",
    "random_choice",
    "random_shuffle",
];

/// Whether calling `name` does something outside the value it answers.
///
/// Normalised before it is looked for, so `files.delete`, `files_delete` and a
/// call through an alias for `files` are one answer rather than three: the dotted
/// spelling is the registry's with a `.` where the registry writes `_`, and
/// [`resolve`] only knows about the latter.
pub fn is_effectful(name: &str) -> bool {
    let qualified = name.replace('.', "_");
    EFFECTFUL_BUILTINS.contains(&resolve(&qualified))
}

/// The names in [`EFFECTFUL_BUILTINS`] the registry does not hold.
///
/// Should always be empty, and is checked rather than trusted for the same
/// reason [`signatures_covering_nothing`] is: a refusal naming a builtin the
/// language does not have refuses something that was never going to run, and
/// reads as a bug in the command rather than as the rule. A module-only name
/// has no flat entry but is still a real builtin, so it is allowed here for the
/// same reason it is allowed in [`signatures_covering_nothing`].
pub fn effectful_names_that_are_not_builtins() -> Vec<&'static str> {
    EFFECTFUL_BUILTINS
        .iter()
        .copied()
        .filter(|name| !BUILTIN_NAMES.contains(name) && !is_module_only_builtin(name))
        .collect()
}

impl Value {
    fn abs(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.abs()),
            Value::Decimal(decimal) => decimal
                .absolute()
                .map(Value::Decimal)
                .unwrap_or(Value::Nothing),
            _ => Value::Nothing,
        }
    }

    fn floor(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.floor()),
            Value::Decimal(decimal) => Value::Decimal(decimal.floor()),
            _ => Value::Nothing,
        }
    }

    fn ceil(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.ceil()),
            Value::Decimal(decimal) => Value::Decimal(decimal.ceil()),
            _ => Value::Nothing,
        }
    }

    fn round(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.round()),
            Value::Decimal(decimal) => Value::Decimal(decimal.round()),
            _ => Value::Nothing,
        }
    }

    /// One of `sin`, `cos` or `tan`. A non-finite answer is `Value::Nothing`
    /// rather than a number no comparison can be trusted on, which is what
    /// `tan` of a pole has to be.
    fn unary(&self, what: &str) -> Value {
        match self {
            Value::Number(n) => {
                let result = match what {
                    "sin" => n.sin(),
                    "cos" => n.cos(),
                    _ => n.tan(),
                };
                if result.is_finite() {
                    Value::Number(result)
                } else {
                    Value::Nothing
                }
            }
            _ => Value::Nothing,
        }
    }
}
