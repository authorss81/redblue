# Phase 075 — Teach the language server navigation: goto-definition, references, completion

## What changed

| File | Lines | What |
|---|---|---|
| src/lsp.rs | +1005 −1 | `Location`, `Definition`, `DefinitionKind`; token-level lexical scope walk; `goto_definition`, `find_references`, `completions` and their JSON forms; greedy string-literal and `//` comment context checks; balanced `else if`, multi-name `set`, typed/`of` parameters, catch binders, object-only `has`, range `to` recognition, decorator-position checks, and function-literal frames that close where the parsed one-line body ends |
| src/lsp.rs | +115 −31 | resumed run: `DeclarationSink` removes the clippy `too_many_arguments` failure without an `allow` suppression, then the review fixes: an assignment `to` is no longer a literal opener, object members resolve in the deferred pass, lexer errors keep their span, string/comment delimiters count as inside, builtin references exclude member/type names, goto excludes member names, and CR/CRLF line starts and comment endings are handled |
| src/lexer.rs | +6 −1 | a lone CR advances the line counter and ends a `//` comment, matching the line breaks the lexer already emits |
| src/lib.rs | +3 −2 | re-export the new navigation surface |
| tests/tooling_grammar_test.rs | +583 −37 | 66 navigation tests (62 `edge_*`) including shadowing, builtins, imports, Unicode, CR/CRLF endings, comments, malformed source and positions, nested `unless`/loop/repeat/module/test/try-catch/object scopes, `else if`, multi-assignment, typed parameters, catch binders, membership `has`, object member access (forward, `this`, ordinary), range bounds, function literals and deterministic navigation over 5000 declarations |
| tests/unreviewed_work_ack_test.rs | +1 −0 | add `resume phase-075:` to the delivery-subject rule so this phase's rebuildable delivery commit is acknowledged by subject, not by a stale SHA |
| src/lsp.rs | +175 −26 | resumed run: navigate object methods. `to can name(...)` declarations are tracked per object, a member followed by `(` resolves to the method while a bare member resolves to the field, and `has`/method declarations get distinct bindings so `has name` and `to can name()` no longer collapse into one entry. Review fixes: a plain-name call prefers a same-scope function over a same-named field. Round-2 fixes: a member or type query answers only its own binding, goto on a type name no longer falls through to a builtin, and an object field is a member read through `this`/the object name rather than a lexical name. |
| tests/tooling_grammar_test.rs | +190 −2 | six new navigation tests: object method calls, `this.method()` calls, field/method same-name separation in both declaration orders, a bare method call over a same-named field, and a malformed-position failure test. |
| src/lsp.rs | +36 −14 | resumed run: model `if`/`unless` branch bodies as their own scope, as the analyzer does. A `set` inside the body writes through to an enclosing binding (or the program scope when the name is new), so it survives `end`, while a function, object or import declared there does not leak. Round-3 review fixes: a `test` body gets its own scope because the harness parses each body separately and runs it in an isolated VM, a `constant` always binds in the program scope (the test body's scope inside a `test`) instead of reusing an enclosing `set`, and the catch scope closes at `finally` so the cleanup body is analyzed in the enclosing scope without the catch binder. |
| tests/tooling_grammar_test.rs | +207 −5 | restored the test-body isolation fixture, corrected the completion test to assert that one test's names do not reach the next, and added navigation tests for an `unless` body whose `set` survives `end` while a declared function does not leak, for a first `set` inside a function or loop being a program binding, for an inner `constant` being a program binding that outlives its loop, and for a `finally` body leaving the catch scope while its own declaration reaches the code after `end`. |
| src/lsp.rs | +8 −3 | resumed run: `find_references` no longer treats an unknown word as a binding. A name with no declaration and no builtin/module/constant resolution has no reference target, so it answers an empty list while a real builtin still answers its unresolved uses. |
| tests/tooling_grammar_test.rs | +34 −0 | three tests: an unknown word has no references; an unknown word beside a real binding leaves that binding's references intact; a zero-column reference query is a clean error. |
| src/lsp.rs | +66 −2 | round-2 review: a `constant` declares its own program-scope binding instead of reusing an earlier `set` of the same spelling, and the deferred pass keeps a name already resolved in source order while still repairing the field/method distinction. |
| tests/tooling_grammar_test.rs | +120 −0 | two tests: `set x` followed by `constant x` keeps the variable and the constant in separate reference sets, and `import MathUtils as M` keeps both spellings as import bindings with separate references. |
| src/lsp.rs | +2 −1 | resumed run: restore the two pieces the preserved tree was missing. The first-`set` program-scope choice tested `DefinitionKind::Set` (a variant that does not exist) instead of `TokenKind::Set`, so a `set` inside a module or `test` was placed in the wrong program scope; and the `Analysis` initializer dropped the `syntax_names` set, so contextual words such as a catch handler or a field `default` could answer as a lexical binding. Both now match the surrounding walk. |
| tests/tooling_grammar_test.rs | +83 −0 | four resumed-run tests: sibling functions with the same parameter name keep separate references; completion before a later declaration does not offer that name; goto on whitespace and on text-literal contents answers `None`; and line 0 is a clean error from all three capabilities. |
| src/lsp.rs | +2 −0 | review: `find_references` also excludes `syntax_names` from both the unresolved-name guard and the candidate match, mirroring `goto_definition`, so a catch failure kind or a field `default` spelled like a builtin no longer answers as that builtin. |
| tests/tooling_grammar_test.rs | +23 −0 | one review test: a catch failure kind spelled `map` neither goes to nor joins the references of the `map` builtin, while the plain builtin use keeps its own reference set. |
| src/lsp.rs | +47 −15 | round-2 review: a builtin use written before a later first `set` or function stays unbound so the builtin fallback answers, while a preceding declaration or a `constant`/`import` still claims the name; object fields no longer enter lexical scopes, matching the analyzer, and member accesses re-resolve after both member tables are complete so `this.name` still prefers a field declared after its method. |
| tests/tooling_grammar_test.rs | +116 −15 | round-2 review tests: a builtin before a later `set abs` keeps the builtin definition and reference set while the variable keeps its own; a bare field read inside a method body has no binding while `this.name` resolves; a position past a line with a builtin and a later `set` is a clean error from all three capabilities; and the field-declared-after-its-method fixture now reads `this.name` and asserts the field's references include that member read. |
| src/lsp.rs | +78 −10 | resumed run: member navigation follows `object Child extends Parent`. The parent name is attached only when it names an object already declared at the child's `object` token, because the interpreter refuses an undeclared parent at declaration time; `member_binding` walks the chain nearest-first, and the lookup guard stops at the first repeated object. A child declaration shadows the same-named ancestor, and `this.member` reads inherited fields. |
| tests/tooling_grammar_test.rs | +228 −22 | seven navigation tests: inherited field goto/references, inherited method references, child-field shadowing with separate parent references, `this` reading an inherited field, a two-level `Child` → `Parent` → `GrandParent` field, a forward `extends` that does not bind because the runtime refuses it, and a malformed-position failure for all three capabilities. |
| src/lsp.rs | +138 −27 | resumed run: a function and a variable of the same spelling share one runtime slot, so navigation now models declaration order. A `to` after a `set` replaces the function binding, a `set` after a `to` starts a variable binding, a `set` inside a block writes to the innermost function/module/test/program frame, and a name resolved during the walk keeps its binding in the deferred pass. `scope_plain_binding` answers the scope's latest entry, and member field/method repair stays in its own post-pass. |
| tests/tooling_grammar_test.rs | +237 −0 | five navigation tests: function-after-variable shadowing with split reference sets, variable-after-function shadowing, a local `set` shadowing an outer function without leaking, a branch `set` writing to the program or enclosing function frame, and a clean out-of-bounds error from goto, references and completion on the shadowing fixture. |
| src/lsp.rs | +183 −29 | round-1 review fixes: a `set` reuses only variable-like slots (variable, parameter, loop variable, catch), so a later `set` after a constant, function, import, object or module starts a fresh variable binding; a called name only falls back to a function declared after the scope's current entry, so a call after a shadowing `set` resolves to nothing; and a method is call-only, so a bare method name answers no lexical binding while `name()` still resolves the method. |
| tests/tooling_grammar_test.rs | +371 −0 | three review tests: `constant` followed by `set` keeps the two declarations in separate reference sets, a call after a `set` that shadows a function resolves to `None` with no references, and a bare method name inside its own body has no binding or references. |
| src/lsp.rs | +18 −4 | round-2 review fixes: a replacing `set` also points the current block scope at its new variable, so a function declared and then shadowed in the same branch body cannot answer from the stale block entry; and a called name only reaches a function declared before the use, so a call after a shadowing `set` does not jump forward to a later `to`. |
| tests/tooling_grammar_test.rs | +85 −0 | two round-2 review tests: a `set` shadows a function declared in the same `if` body while the earlier call keeps the function's references, and a call between a shadowing `set` and a later `to` does not reach the later function. |

No server loop or transport was added: the phase asks for pure functions over source text, and the existing `diagnostics_json` shape is extended per capability.

The resumed run reproduced the write-through symptom against the runtime
before editing: `printf 'unless no then\n    set leaked to 42\nend\nsay leaked\n'`
run through `target/debug/rb run` prints `42`, so a `set` inside an `unless`
body writes to the binding the interpreter still sees after `end`. Navigation
models the analyzer, which pushes and pops a scope around each `if`/`unless`
branch: a `set` inside the body writes through to an enclosing binding (or the
program scope when the name is new), so it survives `end`, but a function,
object or import declared in the body belongs to the body's own scope and does
not leak. The dedicated fixtures pin both halves. A `test` body is different in
the harness: `rb test` parses each body separately and runs it through
`run_isolated`, so a name declared in one test is not visible in another
(verified with two tests where the second reads the first's `x`:
`AnalyzerError: Unknown variable 'x'`). Navigation therefore scopes each
`test` body on its own. A `constant` is not a write-through: the VM binds it to
a global whatever block declares it, and the analyzer declares it in the
program scope, so an inner `constant` outlives its loop (inside a `test`, the
program scope is the test body's scope). The `finally` body was likewise
checked against the runtime: it cannot read the catch binder, and a name it
declares is still bound after `end`.

The final resumed run reproduced a reference defect: for
`say frobnicate\nsay frobnicate\n`, `goto_definition` answered `None` (no
binding exists) while `find_references` answered both uses. An unresolved word
is not a binding, so only a builtin or module name may have file references
without a declaration. The new guard makes the two answers agree, and the
builtin-reference test still passes.

The round-2 review found a real shadowing defect: `set x to 1` followed by
`constant x to 2` marked the constant declaration with the variable's binding,
because `declare` reuses the program scope's existing entry and the deferred
pass then rewrote every non-call use to that first entry. Constants now use a
dedicated program-scope declaration that keeps an existing binding only when it
is also a constant, and the deferred pass no longer overwrites a name the walk
already resolved; it still repairs the field/method shape. The new fixture
asserts that the variable's references stop at the constant and the constant's
references do not join the `set`.

The round-2 review also found that the deferred pass rebound an earlier builtin
use to a later first `set`. The fixture `say abs` followed by `set abs to 1` now
keeps the first `abs` on the builtin registry: the deferred pass only binds a
builtin name to a declaration that precedes the use or is a `constant`/`import`,
which is exactly the analyzer's "a builtin answers until the program claims the
name" rule. The same review questioned the bare field read fixture; running the
analyzer on `object Row` / `has name default "field"` / `to can name()` /
`give back name` reports `Unknown variable 'name'`, so object fields no longer
enter lexical scopes. The fixture now uses `this.name`, and member accesses are
re-resolved once both member tables are complete so a field declared after its
method still answers the member read. A dedicated fixture also pins that a bare
field read has no binding or references, while `this.name` resolves. The new
builtin fixture pins both directions: the builtin reference set contains only
its own use, while the `set` declaration and its later use form a separate
variable binding. A malformed-position fixture covers the same source shape so
an out-of-bounds column stays a clean error from all three capabilities.

The review's import finding did not reproduce. The analyzer explicitly
declares both `item.name` and `item.alias` (`src/analyzer.rs:548-556`), and the
runtime agrees: `import json as J` then `say json.stringify(2)` prints `2`, and
`import files as f` then `say files` prints its (empty) value rather than an
unknown name. Two existing runtime tests pin this contract
(`edge_import_alias_of_a_builtin_namespace_reaches_the_same_function` and
`edge_an_import_alias_leaves_the_modules_own_name_working`), so navigation
declaring both is correct. The new navigation fixture asserts both spellings
resolve to the import and keep separate references; no production code was
changed for that finding.

The resumed run reproduced the remaining navigation gap against the runtime:
for
`object Person` / `has age default 1` / `end` /
`object Employee extends Person` / `end` / `say Employee.age`, the interpreter
prints `1` because `declare_object` copies the parent's fields into the child,
while `goto_definition` on `age` answered `None`. The same held for an
inherited method. Member lookup now walks the `extends` chain nearest-first, so
the declaration span stays on the ancestor that wrote it, a child field shadows
the ancestor, and a two-level chain resolves a field declared on the
grandparent. The parent edge is attached only when the parent object was
already declared at the child's `object` token: `declare_object` resolves the
chain at declaration time and rejects an undeclared parent
(`src/interpreter.rs:3002-3006`), so a forward `extends` is not a parent in
navigation either, and a self-reference is not an edge. The seven new fixtures
pin each half, including `this.age` inside a child method, the grandparent
field, the forward-parent refusal, and a clean out-of-bounds error from all
three capabilities.

The final resumed run reproduced a shadowing defect against the interpreter.
For `set f to 1` / `to f()` / `give back 2` / `end` / `say f`, `rb run` prints
`<function f>`, but `goto_definition` on the function declaration answered the
variable at `1:5` and its references merged both declarations. The reverse
order (`to f()` first, then `set f to 1`) prints `1` for a later `say f` and
raises `RuntimeError: Unknown function 'f'` for a later `say f()`, while
navigation answered the function at `1:4` for the `set`. A third check,
`set f` inside a function that only has an outer `f` function, prints the
function again after the function returns: the assignment created a local
variable, not a write through the function slot. A fourth check puts the `set`
inside an `if` branch: at top level it replaces the global function, while
inside a function it creates that function call's local variable, so a block
scope writes to the innermost runtime frame rather than to the branch's
analyzer scope. A function and a variable of one spelling therefore share a
single runtime slot that the latest declaration replaces. Navigation now gives
the later declaration its own binding, keeps earlier uses on the earlier
binding, and writes a local `set` into the innermost function, module, test or
program frame. The deferred pass no longer rewrites a name the walk already
resolved; member field/method repair remains in its separate post-pass.

The round-1 review found three more declaration-order gaps. A `set` after a
`constant` reused the constant's binding because the reuse filter excluded only
functions; a `set` now reuses only a variable, parameter, loop variable or catch
binder and starts a fresh variable binding after any other kind, so the
constant's references stop at the `set` and the variable's references start
there. A call after `set f` still fell back to the earlier shadowed function
through the same-scope search, even though the runtime raises
`RuntimeError: Unknown function 'f'`; the search now only accepts a function
declared after the scope's current entry (and, after round 2, before the use),
and the deferred pass does not resurrect a name the walk already resolved to
nothing. Finally, a method with
no same-named plain declaration became the scope's entry, so a bare `name`
inside its own body resolved to the method while the analyzer reports
`Unknown variable 'name'`; methods are now call-only, so `name()` resolves the
method and a bare method name answers no binding. The three new fixtures pin
each shape.

The round-2 review found the same-slot model still leaking through a block
scope. For `if yes then` / `to f()` / `set f to 1` / `say f` / `end`, the
function lands in the branch's analyzer scope while the `set` writes to the
program's runtime frame; the lookup then met the branch's stale function entry
before the replacing variable. A replacing `set` now also points the current
block scope at its new variable, so the read after it answers the variable
while the earlier call keeps the function. The review also found that the
same-scope function fallback could jump forward past a shadowing `set` to a
`to` written after the call, even though the runtime holds the variable there
and raises `Unknown function 'f'`; the fallback is now bounded by source
position, and the deferred pass still resolves a genuine call written before a
later declaration. The two new fixtures pin both orders. The gate count below
was re-run and re-harvested: the previous table's `2843` did not match the
suite, so the final number is the 2845 this run actually reported.

## Tests added

| Test | Edge class covered |
|---|---|
| goto_definition_on_variable_use_returns_declaration_span | happy path, exact declaration span |
| edge_goto_definition_on_builtin_resolves_to_registry_not_file_span | builtin registry resolution, no file span |
| find_references_distinguishes_shadowed_bindings | nesting / shadowing (inner parameter vs outer `x`) |
| edge_find_references_on_imported_module_spans_the_import | import binding site, not a module definition file |
| edge_nested_import_stays_in_its_block | nesting: an import declared in a function body resolves there and does not leak past its `end` |
| edge_empty_source_answers_without_failing | empty source |
| edge_goto_definition_counts_unicode_names_as_characters | Unicode (character columns) |
| edge_completions_at_eof_without_trailing_newline_include_keywords_builtins_and_scope | boundary: EOF with no trailing newline |
| edge_completions_at_eof_after_a_one_line_literal_exclude_its_parameters | boundary: a one-line literal closes at the final line |
| edge_reference_after_a_one_line_literal_resolves_to_the_outer_binding | nesting: a literal parameter does not capture a same-line argument after it |
| edge_completions_inside_string_literal_are_empty | context awareness: inside text |
| edge_completions_inside_line_comment_are_empty | context awareness: inside a `//` comment |
| edge_completions_at_the_end_of_a_line_comment_are_empty | boundary: the terminating newline still belongs to the comment |
| edge_completions_at_eof_inside_an_unterminated_comment_are_empty | boundary: an unterminated comment runs to EOF |
| edge_completions_after_a_line_comment_still_offer_scope | a previous-line comment does not close the scope |
| edge_completions_do_not_treat_slashes_inside_text_as_a_comment | `//` inside a text literal is not a comment |
| edge_navigation_rejects_a_column_past_the_line | out of bounds: column past the line; asserts `is_err()` for all three |
| edge_navigation_resolves_a_binding_after_five_thousand_lines | resource: a deterministic goto/references/completion answer at the last of 5000 declarations, with no wall-clock ratio |
| edge_loop_and_object_scopes_do_not_pop_the_enclosing_function_scope | nesting: a loop/object `end` closes only its own scope |
| edge_for_loop_variable_does_not_leak_into_the_enclosing_scope | nesting: a loop variable is scoped to the loop body |
| edge_unless_body_set_survives_the_end_like_the_interpreter | nesting: an `unless` body is its own scope, but a `set` writes through to the program scope, so it survives `end` while the enclosing binding does too |
| edge_unless_body_binding_joins_the_enclosing_bindings_references | nesting: goto and references agree that an `unless` declaration, its body use and its use after `end` are one binding |
| edge_function_declared_in_a_branch_body_does_not_leak_past_end | nesting: a function declared in an `if` or `unless` body stays in that branch's own scope |
| edge_first_set_inside_a_function_is_a_program_binding | nesting: a first `set` in an `unless` body inside a function lands in the program scope and survives the function's `end` |
| edge_first_set_inside_a_loop_is_a_program_binding | nesting: a first `set` in a loop lands in the program scope and survives the loop's `end` |
| edge_navigation_rejects_a_position_past_an_unless_body | out of bounds: line `99` past an `unless` body is a clean `Err` from all three; asserts `is_err()` |
| edge_while_scope_does_not_leak_into_the_enclosing_scope | nesting: a `while` body's declaration stays in its own scope and the enclosing binding survives its `end` |
| edge_repeat_scopes_do_not_leak_into_the_enclosing_scope | nesting: counted `repeat`/`end` and `repeat`/`until` both close only their own scope |
| edge_module_scope_does_not_leak_into_the_enclosing_scope | nesting: a module body's declaration stays inside it while the module name survives its `end` |
| edge_test_block_keeps_the_enclosing_scope_balanced | nesting: a `test` body reads the enclosing scope, a name declared directly in the body and its inner block does not leak, and the enclosing binding survives its `end` |
| edge_test_body_declaration_is_not_offered_by_completion_after_end | nesting: completion after a `test` `end` offers neither the body's own declaration nor its nested loop names, and a later test cannot resolve them |
| edge_constant_in_an_inner_scope_is_a_program_binding | program binding: a `constant` declared in a loop has kind `constant` and outlives the loop's `end`, as in the analyzer and VM |
| edge_finally_body_leaves_the_catch_scope | nesting: the catch binder is gone in `finally`, while a `finally` declaration reaches the code after `end` and the binder still does not leak |
| edge_try_catch_scope_does_not_leak_into_the_enclosing_scope | nesting: a catch binder stays in its own scope and the enclosing binding survives the try's `end` |
| edge_else_if_inside_a_function_keeps_the_function_scope_balanced | nesting: `else if` balances one `If` frame and the function closes its own scope |
| edge_multiple_assignment_declares_every_name_before_to | every name in `set a, b to ...` is declared |
| edge_typed_and_of_parameters_do_not_declare_their_type_names | typed/`of` parameters skip `of`, `list` and `number` |
| edge_catch_binders_are_the_name_after_the_handler_word | plain and typed catch binders, goto and references |
| edge_membership_has_does_not_declare_the_name_after_it | membership `has` is not a property declaration |
| edge_range_with_a_call_lower_bound_does_not_push_a_function_frame | range `to` is not a function declaration |
| edge_range_with_an_lru_cache_lower_bound_does_not_push_a_function_frame | an ordinary `lru_cache` variable before a range's `to` is not a decorator |
| edge_parameterless_function_literal_end_keeps_the_enclosing_scope | a keyword-started literal's `end` closes the literal, not the function around it |
| malformed_positions_are_clean_errors_not_panics | out of bounds / malformed position; asserts `is_err()` |
| edge_malformed_source_returns_errors_not_panics | malformed source: all three surface a lexer error as `Err` |
| navigation_json_extends_the_line_column_shape | JSON `{line, column, ...}` contract for all three |
| edge_goto_definition_on_a_declaration_returns_its_own_span | singleton: goto on the declaration names itself |
| edge_completions_on_an_empty_line_offer_keywords_builtins_and_scope | empty prefix on a blank line offers keywords, builtins and scope |
| edge_completions_after_a_function_end_exclude_its_parameters | nesting: a parameter does not leak past its `end` |
| edge_navigation_rejects_an_enormous_line_number | out of bounds / numeric boundary: `usize::MAX` line is a clean error; asserts `is_err()` |
| edge_assignment_to_inside_an_inline_block_keeps_the_function_scope_balanced | nesting: an assignment's `to` does not open a literal frame that steals the block `end` |
| edge_object_member_access_navigates_a_forward_reference | forward reference: `p.name` above `object p ... has name` resolves and joins the field's references |
| edge_object_member_access_navigates_this_and_ordinary_reference | object navigation: both `this.name` and `Person.name` resolve to `has name` and appear in its references |
| edge_builtin_references_exclude_member_and_type_names | context awareness: `holder.abs` and the type name in `items of abs` are not uses of the `abs` builtin, while the plain `abs` use still resolves to the registry |
| edge_goto_definition_on_a_member_name_does_not_resolve_to_a_builtin | context awareness: goto on `holder.abs` answers `None` while a plain `abs` call still answers the builtin |
| edge_navigation_handles_cr_and_crlf_line_endings | line endings: lone CR and CRLF line starts, comment termination, goto, references and completion after the comment |
| edge_completions_at_the_opening_quote_and_comment_marker_are_empty | boundary: a caret on the opening quote or on the first `/` of `//` is inside text/comment |
| edge_malformed_source_error_carries_the_lexer_span | malformed source: the error keeps both `Unterminated string` and its `1:5` span |
| edge_goto_definition_on_object_method_call_resolves_to_the_method_declaration | object navigation: `Greeter.greet()` resolves to `to can greet`, and its references include the call |
| edge_goto_definition_on_a_this_method_call_resolves_to_the_method_declaration | object navigation: `this.greet()` resolves to the method declaration |
| edge_object_method_and_field_of_the_same_name_stay_distinct | duplicate names: `Row.name` is the field while `Row.name()` is the method, and their references stay separate |
| edge_object_field_declared_after_its_method_still_wins_a_member_read | duplicate names: a `this.name` read resolves to `has name` even when `to can name()` is declared first, while the call still resolves to the method |
| edge_bare_method_call_prefers_the_method_over_a_same_named_field | duplicate names: a bare `b()` call resolves to the method while the field's references stay separate |
| edge_builtin_use_before_a_later_set_stays_the_builtin | duplicate names / deferred declaration: a builtin use before a later `set` stays the builtin, its references stop at the variable declaration, and the variable keeps its later uses |
| edge_navigation_rejects_a_position_past_a_builtin_before_a_later_set | out of bounds: a column past the line is a clean `Err` from goto, references and completion beside a builtin and a later `set` |
| edge_object_field_is_not_a_bare_lexical_name | analyzer modeling: a bare field read inside a method body has no binding or references, matching the analyzer's `Unknown variable` for `give back name` |
| edge_navigation_rejects_a_position_past_the_file | out of bounds: line `99` in a one-line file is a clean `Err` from all three, never a panic |
| edge_find_references_on_an_unknown_name_returns_no_binding | missing binding: an unknown word has no definition and no references |
| edge_find_references_on_an_unknown_name_beside_a_real_binding_still_answers_the_binding | missing binding beside a real binding: the unknown word does not join the binding's references |
| edge_find_references_rejects_a_zero_column | out of bounds: column 0 is a clean `Err` |
| edge_constant_after_a_same_named_set_is_its_own_binding | duplicate names: a `constant` after a same-named `set` is a distinct binding, so the variable's references stop at the constant and the constant's references do not join the `set` |
| edge_import_alias_keeps_the_modules_own_name_a_binding | import alias: `import MathUtils as M` binds both `MathUtils` and `M`, each navigating to the import with separate references |
| edge_references_in_sibling_functions_do_not_cross | nesting / duplicate names: two functions with a parameter of the same spelling keep separate reference sets |
| edge_completion_before_a_later_declaration_does_not_offer_it | boundary: a declaration below the caret is not in scope yet, while keywords are still offered |
| edge_goto_definition_on_whitespace_and_text_returns_none | boundary: whitespace and a text literal's body name no binding |
| edge_navigation_rejects_line_zero_for_every_capability | out of bounds / malformed position: line 0 is a clean `Err` from goto, references and completion |
| edge_syntax_names_do_not_answer_as_builtin_references | context awareness: a catch failure kind spelled like the `map` builtin is syntax, so goto answers `None` and references stay with the plain builtin use |
| edge_goto_definition_resolves_an_inherited_field | inheritance: `Employee.age` resolves to `Person`'s `has age`, and both uses join one reference set |
| edge_find_references_on_an_inherited_method_spans_the_parent_declaration | inheritance: `Employee.greet()` joins `Person`'s `to can greet` declaration |
| edge_inherited_member_shadowing_prefers_the_child_declaration | duplicate names / inheritance: the child field shadows the parent field, and the parent keeps its own references |
| edge_this_member_resolves_through_the_parent_chain | inheritance: `this.age` inside a child method resolves the ancestor's field |
| edge_goto_definition_resolves_a_grandparent_field | inheritance: a two-level `Child` → `Parent` → `GrandParent` chain resolves the field on the grandparent and joins its references |
| edge_extends_does_not_bind_a_parent_declared_after_the_child | malformed input: a forward `extends` matches the interpreter's refusal and answers `None` with no references |
| edge_inherited_member_navigation_rejects_a_column_past_the_line | out of bounds: a column past the line is a clean `Err` from all three capabilities on an inheritance fixture |
| edge_function_declaration_shadows_an_earlier_variable | duplicate names: a later `to` replaces an earlier `set` in the shared runtime slot; references split at the declaration |
| edge_later_set_shadows_an_earlier_function | duplicate names: a later `set` replaces an earlier `to`; the function's earlier use keeps its own reference set |
| edge_set_inside_a_function_shadows_an_outer_function_locally | nesting: a local `set` shadows an outer function inside its scope and does not leak past `end` |
| edge_set_inside_a_branch_shadows_a_function_in_the_runtime_frame | nesting: a branch `set` writes through to the program frame at top level and to the enclosing function's frame inside a function |
| edge_function_variable_shadowing_rejects_a_position_past_the_line | out of bounds / failure: a column past the line is a clean `Err` from goto, references and completion on the shadowing fixture |
| edge_set_after_a_same_named_constant_is_its_own_binding | duplicate names: a `set` after a same-named `constant` is a distinct variable binding, so the constant's references stop at the `set` and the variable's references do not join the constant |
| edge_call_after_a_set_shadowing_a_function_does_not_resolve | duplicate names: a call after `set f` does not resolve to the shadowed function, matching the runtime's `Unknown function 'f'`, while the earlier function keeps only its earlier uses |
| edge_method_name_is_not_a_bare_lexical_name | analyzer modeling: a bare method name inside its own body has no binding or references, matching the analyzer's `Unknown variable` for `give back name` |
| edge_set_shadows_a_function_declared_in_the_same_branch | duplicate names / nesting: a function declared in an `if` body and then `set` in the same body gives the read after the `set` to the variable, while the earlier call keeps the function's references |
| edge_call_after_a_set_does_not_reach_a_later_function | duplicate names / declaration order: a call between a shadowing `set` and a later `to` resolves to nothing and joins neither declaration's references |

### Mandatory edge-case matrix

| Row | Covered? |
|---|---|
| empty | covered by `edge_empty_source_answers_without_failing` |
| singleton | covered by the one-use fixtures; a single reference is asserted exactly |
| boundary | covered by EOF-without-newline; cursor at column `len + 1`; end-of-comment and unterminated-comment positions; lone-CR and CRLF line endings |
| out_of_bounds | covered by `malformed_positions_are_clean_errors_not_panics` (line 0, line past EOF, column 0, column past line), by `edge_navigation_rejects_a_column_past_the_line`, and by `edge_navigation_rejects_an_enormous_line_number` (`usize::MAX`) |
| type_mismatch | N/A: navigation consumes source text and positions only, never values |
| numeric_boundary | N/A: no arithmetic or numeric values are evaluated |
| unicode | covered by `edge_goto_definition_counts_unicode_names_as_characters` (span is four characters, not five bytes) |
| nesting_recursion | covered by the shadowing test (function scope nested in top-level scope), by the loop/object-in-function tests, and by dedicated `unless`, `while`, counted/until `repeat`, `module`, `test` and `try`/`catch`/`finally` fixtures. `if`/`unless` branch bodies open their own scope, matching the analyzer: a `set` inside writes through to the enclosing/program binding and survives `end`, while a function, object or import declared there does not leak. A `test` body gets its own scope because the harness runs each body in isolation; loops, functions, modules, objects, test bodies and catch binders open their own scope, a `constant` is always a program binding (the test body's scope inside a `test`), the catch scope closes at `finally`, and a nested `unless` inside a function stays inside that function |
| duplicate_missing_keys | records/maps are N/A because navigation never reads values; same-spelling declarations are covered: a repeated `set` reuses its binding, while a later `set`, `constant` or `to` replaces a binding of another kind and keeps the earlier uses distinct (`edge_function_declaration_shadows_an_earlier_variable`, `edge_later_set_shadows_an_earlier_function`, `edge_constant_after_a_same_named_set_is_its_own_binding`) |
| malformed_input | covered by `edge_malformed_source_returns_errors_not_panics` (unterminated string surfaces `Err` from all three) and by `malformed_positions_are_clean_errors_not_panics`; comment/string context checks cover `//` inside text |
| resource_limit | covered by `edge_navigation_resolves_a_binding_after_five_thousand_lines`: a deterministic, fixed 5000-line source answers goto/references/completion at its last declaration; no wall-clock ratio is asserted |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2845 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh "phase-075" | pass |

`cargo test --all-targets` ran 85 test binaries and reported 2845 passed, 0
failed and 0 ignored. This is the actual count from the final round-2 gate run,
including the ten tests added since the committed revision (the eight round-1
fixtures and the two round-2 fixtures); the earlier `2843` table value was not
what the suite reported.

## Invariants touched

- None to the public `Value`, `Error`, grammar or `.rb` extension. The only public additions are new `redblue::lsp` items re-exported from `redblue`.
- Scope modeling now follows the analyzer and the runtime together: an `if`/`unless` branch body is its own scope, but a `set` inside it writes through to an enclosing binding (or the program scope when the name is new), so it survives `end` while a function, object or import declared there does not. Each `test` body is scoped on its own because the harness runs it in an isolated VM; the previous attempt modeled both as shared, so a name could leak from one test to the next, and the restored isolation fixture pins that distinction. A `constant` always binds in the program scope (the test body's scope inside a `test`), and the catch scope closes when `finally` starts.
- A function and a variable of the same spelling share one slot in the runtime, so a later `set`, `constant` or `to` replaces a binding of another kind. Navigation now models that declaration order: the later declaration owns the following uses, while the earlier declaration keeps its own uses. An inner `set` over an outer function creates a local variable rather than writing through the function. No test from `main` was deleted or weakened.

## Known gaps / follow-ups

- The functions implement capabilities only; the LSP transport/server loop remains phase-017 scaffolding by design.
