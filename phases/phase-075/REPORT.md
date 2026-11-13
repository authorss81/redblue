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
| src/lsp.rs | +108 −1 | resumed run: member completion. The analysis keeps its object field/method/parent tables and the `this`-to-object map, and `member_completions` answers a `receiver.member` slot with the object's own and inherited members or a standard module's published members, filtered by the partial member name. The ordinary lexical answer still runs for every position that is not a member slot. |
| tests/tooling_grammar_test.rs | +125 −0 | five tests: object fields and methods after `Name.` with prefix filtering; `this.` inside a method; inherited members under `extends`; module members by their dotted spelling rather than the `files_read` registry encoding; and a clean out-of-bounds error in a member slot. |
| src/lsp.rs | +65 −32 | round-1 review fixes for member completion: the analysis records an import name or alias to the module it names, mirroring the runtime's `module_aliases`, so `import files as f` followed by `f.` looks up `MODULE_MEMBERS` for `files`; and completion recognises a syntactic member slot before resolving the receiver, then answers from the complete walk, so `Person.` above `object Person` offers the object's members instead of falling back to keywords. A member slot whose receiver is neither an object nor a standard module now answers empty rather than offering the lexical registry. |
| tests/tooling_grammar_test.rs | +63 −0 | two round-1 review tests: an aliased module member slot offers its published members and no keywords, and an object member slot above the object's declaration offers its field and method and no keywords. |
| src/lsp.rs | +102 −2 | round-2 review: the analysis records each `module … end` body's top-level `to`, `set` and `constant` names and its last `export`, keyed by the module binding, and filters them into a declared-module member table when the module closes. `member_completions` consults that table before `MODULE_MEMBERS`, so a program-declared module (and an import of it) offers what it publishes even when its name is also a stdlib module's. The flat alias table is now consulted only for a receiver that has an in-scope `Import` binding, so an alias written inside a function or branch cannot answer after its `end`. |
| tests/tooling_grammar_test.rs | +106 −0 | four round-2 review tests: `module text … end` offers its exported members rather than the stdlib `text` members; an explicit `export` filters a declared module's names; an import alias of a declared module offers its members; and an import alias used after its function or branch block offers nothing. |
| src/lsp.rs | +39 −7 | resumed run: goto-definition and find-references follow a call to a program-declared module's published function. Each module body's top-level `to` records its binding, the module's last `export` filters the published functions when the body closes, and the deferred member pass maps `Module.name()` and `alias.name()` to that declaration. An unexported name and a standard module's member stay unresolved, matching the runtime. |
| tests/tooling_grammar_test.rs | +113 −0 | four resumed-run tests: goto on `MathUtils.circle_area` inside a declared module; references joining the declaration, two bare calls and an aliased member call; an explicit `export` resolving the published function while the hidden one answers `None` with no references; and a clean `Err` from all three capabilities at line 0, past the line and past the file. |
| src/lsp.rs | +30 −9 | final round-1 review fixes: the published-function table is keyed by the module binding rather than its name, so a redeclaration or a same-named module in another block cannot answer for the first module's calls, and a module declaration gives a redeclaration its own binding; a module-top-level `set` removes the same-named function entry because it replaced the member value (the round-3 review corrected the `constant` half: a module `constant` binds a program global and leaves the function published); and a bare `Module.name` read resolves through the same table as a call, matching the runtime's exposed function value. |
| tests/tooling_grammar_test.rs | +130 −0 | four final round-1 review tests: two same-named module declarations keep their own functions; a `set` that shadows a module function does not navigate to the stale `to` (the `constant` half was corrected in round 3); and a bare module-member read resolves to the published function and joins its references. |
| src/lsp.rs | +42 −10 | round-2 review fixes: a `set` inside a nested block removes the replaced module function by finding the innermost runtime frame's module rather than looking only at the top frame (the round-3 review narrowed this from `set` or `constant` to `set`, because a constant is a global), and an `import` records the module binding it named at its own site, so an alias written before a later same-named module keeps the earlier module's functions and completed members. |
| src/lsp.rs | +26 −0 | resumed run: a member write (`set Person.name to …`) no longer declares its receiver as a fresh variable. The `set`/`constant` target walk stops at a following `.`, leaving the receiver to the ordinary identifier pass and the written member to the member pass, so goto-definition on either side resolves the object and its field and the object keeps its program-scope binding for later reads. A write whose receiver resolves to a declared module or import alias also removes the replaced member from that module's published-function table before the write is resolved, so neither the write nor a later read resolves to the stale `to`. |
| tests/tooling_grammar_test.rs | +221 −0 | six navigation tests for member writes: the receiver resolves to the object declaration and the member to `has name`, a later read still resolves the object rather than a shadowing variable, a write inside a branch is a reference to the program-scope object and field, malformed positions remain clean `Err`s, and `set M.f`/`set A.f` through a module or an import alias drops the replaced published function while an earlier read still resolves it. |
| tests/bench/mod.rs | +74 −5 | gate repair for a pre-existing cross-target defect: the `rb-bench` binary's own unit tests live under `target/<profile>/build/…`, where the old sibling lookup could not find the runner. The lookup now resolves `target/<profile>/rb-bench` and the cross-target `target/<triple>/<profile>/rb-bench`, so the child-process measurement remains real instead of failing with "cannot locate the rb-bench worker binary" or measuring a stale binary elsewhere; a fixture pins both layouts. |
| phases/phase-088/scan.rs | +1 −4 | gate repair for a pre-existing clippy `needless_bool` diagnostic in an unrelated phase's scanner; the expression is unchanged in meaning. |
| tests/tooling_grammar_test.rs | +99 −0 | three round-2 review tests: a nested branch's `set` drops the stale module function (the `constant` half was corrected in round 3); an alias between two same-named modules resolves and completes from the module its import named, not the later declaration. |
| src/lsp.rs | +260 −0 | resumed run and round-1 review: completion inside a `{…}` text interpolation. The literal is decoded first, so `\u007B`/`\u007D` are braces to the LSP exactly as they are to the lexer; a pair is interpolation only when its contents re-parse as one complete expression, so a JSON object in a string stays text; `this` is offered as an expression keyword; and a `receiver.member` slot inside a pair is re-analyzed in the literal's enclosing scopes so it answers the object's or module's members rather than a flat lexical list. Text outside a pair, and a closed pair, still answer nothing. |
| tests/tooling_grammar_test.rs | +160 −0 | eight interpolation tests: scope and builtins with no statement keywords; text before and after a pair in the same literal; a later declaration not offered above the caret; a malformed column/line producing a clean `Err`; JSON braces that are not an expression stay text; `this` is offered; `{this.}` offers the object's members; and a `\u007B` escape opens an interpolation. |
| src/parser.rs | +18 −1 | round-1 review: `is_complete_expression` and a crate-visible `unmatched_opens` expose the parser's own interpolation rule, so the LSP asks the same question instead of trusting brace balance alone. |
| src/lsp.rs | +159 −4 | resumed run: goto-definition and find-references now look inside a `{…}` text interpolation, closing the follow-up recorded by the previous attempt. The literal is decoded and re-analyzed with the interpolated expression in place of the text token; decode positions are mapped back to the original source so a use reports the caret the editor wrote, even when the braces were `\u007B`/`\u007D` or the expression sits in a nested function scope. A name in text that is not a substitution still answers `None`/no references. |
| tests/tooling_grammar_test.rs | +139 −0 | six resumed-run tests: goto inside an interpolation returns the outer declaration's exact span; references join the declaration and the interpolated use; a builtin inside an interpolation resolves through the registry; malformed positions remain clean errors; escaped braces map back to the original span; and an interpolated use prefers the shadowing local over an outer function. |
| src/lsp.rs | +159 −6 | round-1 review: find-references is now interpolation-aware for the whole file instead of only for a query that starts inside text. Every text literal's interpolation is re-analyzed, a name that resolves to the queried declaration is kept, and its mapped use is merged with the main token-stream answer, so the declaration-side query and the inside-text query return the same set. The old single-literal path remains only for a name declared inside the literal itself, where the main analysis has no declaration to match against other literals. |
| tests/tooling_grammar_test.rs | +38 −0 | two round-1 review tests: a declaration-side query finds the use inside a text interpolation, and it reaches uses in every text literal in the file rather than only one. |
| src/lsp.rs | +236 −34 | round-2 review: brace pairing is a single stack pass over the decoded literal, candidates are tested from the inside out under a bounded work budget, a valid pair skips the span it replaces exactly as the parser does, and literals whose decoded text cannot contain the queried name are skipped. All substitutions in one literal are re-analyzed together instead of analyzing the whole file once per pair; that analysis is capped per query and reused for every use in the literal. Nested text literals inside an interpolated expression are walked through the resulting view, so their substitutions still contribute references. |
| tests/tooling_grammar_test.rs | +90 −0 | round-2 review tests: interpolation references now assert `end_line`/`end_column` for the declaration, a use followed by text, a use that fills the expression, and an escaped-brace use; a builtin inside `{abs(-3)}` is queried with `find_references` and maps back to its source span. |
| src/lsp.rs | +42 −3 | resumed run: a declared module's published `set` values now navigate. Each `module … end` body's top-level value bindings are recorded and filtered by its last `export`, a later `to` of the same name replaces the value, and a bare `Module.name` read resolves to the value declaration while a `Module.name()` call stays unresolved because a value is not callable. |
| tests/tooling_grammar_test.rs | +150 −0 | five module-value navigation tests: goto on an exported `set` member, references keeping a module value apart from a same-named program variable, export filtering, a later function replacing the value, and malformed positions as clean `Err`s. |
| src/lsp.rs | +20 −8 | round-3 review: the owning module for a published value is the innermost runtime frame's module, so a replacing `set` inside an `if`/loop in a module body is recorded exactly as the function-removal walk already found it; and a `constant` is no longer treated as a module value at all. The VM binds a module `constant` to a program global, so it is neither published by the namespace nor a replacement for a same-named module function. |
| tests/tooling_grammar_test.rs | +50 −20 | round-3 review tests: a `set` written in a nested branch resolves a later bare `M.f` and shares its references; a module `constant` does not replace the published function at the body's top level or inside a branch; and the module-value fixture now asserts a `constant` member answers `None` with no references. |
| tests/module_system_test.rs | +24 −0 | round-3 golden test: a module's `set` is a published member while its `constant` is a program global the module does not publish, pinned against the runtime's own member table. |
| src/lsp.rs | +10 −4 | round-2 review: a `to` is published only when its declaration frame is the `Module` frame itself, so a function written inside a branch or loop is not a module body item; and the `set` publication owner is the same innermost runtime frame the value table uses, so a nested-branch `set` is both appended to `module_declared` and kept by the filter that builds `declared_module_values`. |
| tests/tooling_grammar_test.rs | +76 −0 | two round-2 review tests: a function declared inside a module branch is not published, so `M.f()` answers `None`, has no references and is not offered by completion; and a module `constant` after a same-named `set` leaves the published value navigable and shares its references. |

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
later declaration. The two new fixtures pin both orders. The gate count is
re-harvested from the current `cargo test --all-targets` run: 2911 passed, 0
failed and 0 ignored across 85 test binaries. The four test instances added in
that earlier round were the two module-member-write fixtures plus the
cross-target worker-layout fixture, which is compiled into both the `rb-bench`
unit harness and `tests/bench_test.rs`. The worker-lookup repair is also what
moved the two `edge_child_measurement_accepts_an_explicit_environment`
instances (the `rb-bench` unit harness and `tests/bench_test.rs`) from failing
to passing; this resumed delta then added the five module-value navigation
tests, the round-3 review added the nested-set fixture and the runtime golden
test, and this round-2 review added the branch-function refusal and the
constant-after-set fixture, taking the run to 2911.

The final resumed run reproduced a module-navigation gap against the runtime.
For
`module M` / `to f()` / `give back 41` / `end` / `export f` / `end` /
`say M.f()`, `rb run` prints `41`, but `goto_definition` on the member `f`
answered `None` and `find_references` answered an empty list; completion
already offered the published member. A program-declared module is a namespace
the runtime built, so its exported function call must navigate like an object
method call. Navigation now records each module body's top-level `to` binding,
filters the functions by the module's last `export` when the body closes, and
resolves `Module.name()` and `alias.name()` to that declaration in the deferred
member pass. A standard module keeps its unresolved answer (it has no file
span), and an unexported function stays unreachable, matching the runtime's
`Module 'M' has no function 'hidden'`. The four fixtures pin the exported and
aliased calls, the reference set joining bare and member calls, the export
filter in both directions, and clean errors for malformed positions.

The final round-1 review found that navigation keyed the published-function
table by module name rather than by the module binding. Two same-named
declarations therefore overwrote one entry when the second body closed, so a
call bound to the first module could jump into the second module's function or
answer nothing. The table is now keyed by the module binding, a redeclaration
gets its own binding, and an import name is mapped to the binding a later
`import` names. The same review found that a module-top-level `set` after a
`to` left the old function in the published table even though it replaced the
member value; that entry is now removed. The round-3 review settled the
`constant` half against the runtime: a module `constant` is bound as a program
global, so the namespace keeps publishing the function. Finally, a bare
`Module.name` read now resolves to the published function exactly as a call
does, because the interpreter exposes the same value either way. Four
fixtures pin the two declarations, the replacement and the bare read.

The round-2 review found two remaining holes in the module tables. A branch
opens an analyzer scope but no runtime frame, so a `set` written inside
`if … end` in a module body replaces the module's value while the published
table still mapped `M.f()` to the old `to f`; the removal now walks to the
innermost frame that creates variables and finds the module whose scope the
write reaches.

The round-3 review found that the module-value table used the top frame rather
than that same innermost runtime frame, so a replacing `set` inside a branch
was never recorded and a later bare `M.f` answered `None`. It uses the
runtime-frame module now, and the new fixture pins the nested replacement. The
same review showed that a module `constant` is a program global: the runtime
member table never holds it, and it does not replace a published function.
Navigation now records and removes only `set` values, and a runtime golden test
asserts the member table holds the module's `set` and not its `constant`. Also, an import alias resolved through the last-wins module
name table, so an alias written between two same-named modules could answer
from the later declaration. Each import now records the module binding it
named at its own site, keyed by the import binding, and both goto-definition
and member completion prefer that binding over the name fallback used when an
import precedes its module declaration. The three new fixtures pin the nested
replacement, the alias between same-named modules and its completed members.

This round-2 review found that the two module tables still disagreed about the
owner. A `to` was attributed to whichever frame was on top, but only a
declaration written directly in the module body is a module body item, so the
owner now comes from a frame that is itself the `Module` frame; a function
inside an `if`/loop is neither published by `export all` nor offered by
completion. The `set` publication filter now uses the same innermost runtime
frame as the value table, so a `set` written inside a branch is appended to
`module_declared` and survives the filter that builds
`declared_module_values`. The two new fixtures pin the branch function's
refusal and a module `constant` written after a same-named `set` leaving the
published value intact.

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
| edge_completions_after_an_object_dot_offer_its_fields_and_methods | boundary / context awareness: `Name.` offers only the object's fields and methods, and a partial member keeps the names it can continue |
| edge_completions_after_this_dot_offer_the_objects_members | context awareness: `this.` inside a method names the enclosing object's members |
| edge_completions_after_a_child_object_dot_include_inherited_members | nesting / inheritance: a child's member slot includes the parent's members and offers no builtins |
| edge_completions_after_a_module_dot_offer_its_published_members | context awareness: a module offers its dotted members, not the registry encoding or language keywords |
| edge_completions_after_a_declared_module_dot_offer_its_published_members | context awareness: `module text … end` offers its own exported members, not the stdlib module of the same name or language keywords |
| edge_completions_after_a_declared_module_only_offer_exported_names | context awareness: a declared module offers only the names its `export` publishes |
| edge_completions_after_importing_a_declared_module_alias_offer_its_members | context awareness: an import alias of a program-declared module offers its published members |
| edge_completions_after_a_block_do_not_use_its_import_alias | nesting / scope: an alias declared in a function or branch does not answer after its enclosing block closes |
| edge_member_completion_rejects_a_position_past_the_line | out of bounds / failure: a column past the line in a member slot is a clean `Err` |
| edge_goto_definition_on_a_declared_module_member_resolves_to_its_declaration | module navigation: `MathUtils.circle_area(2)` resolves to the `to circle_area` inside the module |
| edge_find_references_on_a_declared_module_member_spans_every_use | module navigation / references: the declaration, two bare calls and an aliased `M.circle_area()` call share one binding |
| edge_declared_module_member_navigation_respects_export | missing binding / export: an unexported member answers `None` with no references, while the explicitly exported function resolves |
| edge_declared_module_member_navigation_rejects_a_malformed_position | out of bounds / failure: line 0, a column past the line and a line past the file are clean `Err`s from all three capabilities |
| edge_same_named_module_declarations_keep_their_own_functions | duplicate names: two `module M` declarations each keep the function their call is bound to, so the second close does not overwrite the first table |
| edge_module_member_shadowed_by_set_does_not_resolve_to_the_stale_function | duplicate names: a module `set f` replaces the published function value, so `M.f()` no longer navigates to the shadowed `to f` |
| edge_a_module_constant_does_not_replace_the_published_function | duplicate names: a module `constant f` binds a program global, so the module still publishes its function |
| edge_a_nested_block_set_replaces_the_published_function_but_a_constant_does_not | nesting / duplicate names: a `set` inside an `if` body reaches the module's runtime scope and replaces the function, while a `constant` there stays a global and leaves the function published |
| edge_aliased_same_named_module_declaration_keeps_the_imported_modules_functions | duplicate names / import alias: an alias between two `module M` declarations resolves and joins the module its import named, not the later declaration |
| edge_completions_after_a_same_named_module_alias_keep_the_imported_modules_members | duplicate names / completion: a same-named module declared after an import does not change the members its alias offers |
| edge_bare_module_member_read_resolves_to_the_published_function | module navigation: a bare `M.f` read resolves to the published function and joins its declaration and export references |
| edge_module_member_write_replaces_the_published_function | module navigation / duplicate names: `set M.f` removes the replaced published function, so the write and a later `M.f` answer `None` while an earlier read still resolves the `to f` |
| edge_import_alias_member_write_replaces_the_published_function | module navigation / import alias: `set A.f` through `import M as A` replaces the same published function, and an earlier aliased read still resolves it |
| edge_worker_binary_resolves_a_cross_target_layout | build harness / boundary: the worker lookup finds both `target/<profile>/<name>` and `target/<triple>/<profile>/<name>` (the fixture runs in the `rb-bench` unit harness and `tests/bench_test.rs`) |
| edge_completions_inside_an_interpolation_offer_expression_names | context awareness: `{…}` inside text is code, so scope names and builtins are offered while statement keywords are not |
| edge_completions_outside_an_interpolation_in_the_same_literal_are_empty | context awareness / boundary: text before and after a balanced pair in the same literal is still text |
| edge_completions_inside_an_interpolation_do_not_offer_later_declarations | scope / declaration order: a declaration below the caret is not in scope above it |
| edge_completions_inside_an_interpolation_reject_a_malformed_position | out of bounds / failure: a column past the line is a clean `Err`, never a panic |
| edge_goto_definition_inside_an_interpolation_resolves_to_the_outer_binding | context awareness / goto-definition: a name inside `{…}` resolves to the `set` declaration outside the literal |
| edge_find_references_inside_an_interpolation_join_the_binding | context awareness / references: the declaration and the interpolated use share one binding while surrounding text is not a use |
| edge_goto_definition_on_a_builtin_inside_an_interpolation_resolves_to_the_registry | builtin registry resolution inside an interpolated expression |
| edge_navigation_inside_an_interpolation_rejects_malformed_positions | out of bounds / failure: line 0, a column past the line and a line past the file are clean `Err`s |
| edge_navigation_inside_escaped_interpolation_braces_maps_the_original_span | Unicode / escapes: `\u007B`/`\u007D` braces answer at the source position the editor wrote |
| edge_interpolated_use_prefers_the_shadowing_binding | nesting / shadowing: an interpolated use resolves the local binding, not an outer same-named function |
| edge_find_references_from_the_declaration_include_interpolated_uses | context awareness / references: the normal declaration-side query finds the use inside a text interpolation |
| edge_find_references_from_the_declaration_reach_every_text_literal | context awareness / references: every text literal in the file contributes its interpolated uses, not only the one nearest the query |
| edge_completions_inside_balanced_braces_that_are_not_an_expression_are_empty | context awareness: a JSON object in a string is text to the parser, so completion stays empty |
| edge_completions_inside_an_interpolation_offer_this | context awareness: `this` is an expression keyword a method may write, so it is offered inside a pair |
| edge_completions_after_a_member_dot_inside_an_interpolation_offer_members | context awareness / member completion: `{this.}` offers the enclosing object's fields and methods, not the lexical registry |
| edge_completions_treat_unicode_escaped_braces_as_interpolation | Unicode / escapes: `\u007B` opens an interpolation exactly as the lexer reads it |
| edge_member_assignment_receiver_resolves_to_the_object_declaration | member write / references: `Person` on the left of `set Person.name` is the object declaration, and `name` is its `has` field; both reference sets include the write |
| edge_member_assignment_does_not_shadow_the_object_for_later_reads | duplicate names / scope: the member write does not replace the object binding, so the later read and the write share the object and field references |
| edge_member_assignment_inside_a_branch_writes_through_to_the_object | nesting: a member write inside an `if` body reaches the program-scope object and field |
| edge_member_assignment_navigation_rejects_a_malformed_position | out of bounds / failure: line past the file, column past the line and line 0 are clean `Err`s from goto-definition, find-references and completion |
| edge_goto_definition_on_a_declared_module_value_resolves_to_its_declaration | module navigation / exact span: `Counter.count` resolves to the module's `set`, while `Counter.label` answers `None` because a `constant` is a program global |
| edge_goto_definition_on_a_module_value_written_in_a_nested_block_resolves_to_it | nesting / module navigation: a `set` inside an `if` body is the module's published value, so a later bare `M.f` resolves to it and shares its references |
| edge_a_module_constant_is_a_global_and_not_a_published_member | runtime golden: the module member table holds its `set` and not its `constant` |
| edge_find_references_on_a_declared_module_value_do_not_cross_a_same_named_variable | duplicate names / shadowing: the module value and a same-named program variable keep separate reference sets |
| edge_declared_module_value_navigation_respects_export | module navigation / boundary: an unexported value answers `None`/no references while an explicitly exported one resolves |
| edge_a_module_function_after_a_value_replaces_the_published_binding | duplicate names / declaration order: a later `to` replaces the module value, so the call jumps to the function |
| edge_module_value_navigation_rejects_a_malformed_position | out of bounds / failure: line 0, a line past the file and a column past the line are clean `Err`s from all three capabilities |
| edge_a_function_declared_in_a_branch_is_not_a_published_module_member | module navigation / nesting: a `to` inside an `if` body is not a module body item, so `M.f()` answers `None`, has no references, and completion does not offer it |
| edge_a_module_constant_after_a_same_named_set_leaves_the_value_published | duplicate names / module navigation: a `constant` after a module `set` is a program global, so `M.f` still resolves to the set and shares its references |

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

The round-1 review found four gaps in interpolation completion. A balanced
`{…}` is not enough to make text code: the parser only treats it as a
substitution when the inner text re-parses as one complete expression, so the
JSON document in `json.parse("{ \"name\": \"Alice\" }")` stays text. The
LSP now calls the parser's own `is_complete_expression` rule over the decoded
literal, which also makes `\u007B` and `\u007D` braces match the lexer's view.
`this` joined the expression keywords so the SPEC's `say "I'm {this.name}"`
can be completed. Finally, a member slot inside a pair is re-analyzed as the
decoded expression in a copy of the source, which keeps the enclosing method,
object and module scopes and lets `{this.}`, `{person.}` and `{files.}` answer
their members. The four new fixtures pin each half, including the JSON case that
would otherwise be offered the lexical registry.

The round-2 review found that a member write to a declared module still left the
replaced function in the published table: the write broke out of the
`set`/`constant` walk before the module bookkeeping, so the deferred pass kept
resolving `set M.f`, later `M.f` reads and `M.f()` calls to the stale `to f`.
The deferred member pass now records member-write tokens and removes the written
member from that module's (or import alias's) published-function table at the
write's source position, so earlier reads keep the function and later uses do
not. Two fixtures cover the direct module and the import alias. The review also
found the benchmark worker lookup only understood `target/<profile>/…`; it now
tries both that layout and `target/<triple>/<profile>/…`, with a fixture. Both
out-of-scope gate repairs remain recorded in
`docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md` as `F-073-02` and `F-088-01`; their rows
now say `[RESOLVED — verify in owner phase]` because the owner phases, not this
phase, must confirm them.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2911 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh "phase-075" | pass |

`cargo test --all-targets` was re-run on this checkout: 2911 passed, 0 failed
and 0 ignored. This resumed delta added five module-value navigation tests to
the harvested total, the round-3 review added the nested-set fixture and the
runtime golden test, and this round-2 review added the branch-function refusal
and the constant-after-set fixture; 2902 + 5 + 2 + 2 = 2911. The two out-of-scope repairs
(the benchmark worker lookup in `tests/bench/mod.rs` and the `needless_bool`
clippy diagnostic in `phases/phase-088/scan.rs`) are retained only because each
was a pre-existing failure of a mandatory gate; the ledger records them for the
owner phases to verify rather than silently closing them here.

## Invariants touched

- None to the public `Value`, `Error`, grammar or `.rb` extension. The only public additions are new `redblue::lsp` items re-exported from `redblue`.
- Scope modeling now follows the analyzer and the runtime together: an `if`/`unless` branch body is its own scope, but a `set` inside it writes through to an enclosing binding (or the program scope when the name is new), so it survives `end` while a function, object or import declared there does not. Each `test` body is scoped on its own because the harness runs it in an isolated VM; the previous attempt modeled both as shared, so a name could leak from one test to the next, and the restored isolation fixture pins that distinction. A `constant` always binds in the program scope (the test body's scope inside a `test`), and the catch scope closes when `finally` starts.
- A function and a variable of the same spelling share one slot in the runtime, so a later `set`, `constant` or `to` replaces a binding of another kind. Navigation now models that declaration order: the later declaration owns the following uses, while the earlier declaration keeps its own uses. An inner `set` over an outer function creates a local variable rather than writing through the function. No test from `main` was deleted or weakened.
- A member write (`set Receiver.member to …`) is a use of `Receiver` and a reference to the field, never a new variable named after the receiver. The interpreter mutates the object/module, and navigation now models that instead of letting the assignment shadow the object binding.

## Known gaps / follow-ups

- The functions implement capabilities only; the LSP transport/server loop remains phase-017 scaffolding by design.
- Completion, goto-definition and find-references now answer inside a `{…}` string interpolation. Find-references from a declaration outside text walks every text literal in the file and merges the uses in each interpolation (including a nested one) that resolve to the same declaration. The remaining asymmetry is a query that starts inside text when the name itself is declared inside text: that answer comes from re-analyzing the one literal, because the main token stream cannot see the inner declaration to match it against other literals.
- Completion enumerates a program-declared module (and an `import` of it) because its `module … end` body is in the source text the navigation functions receive. A module loaded from a separate file has no declarations in that text, so these pure-source functions cannot enumerate its members; `Vm::module_member_values` can do so once the runtime has loaded the file. Reading module files here would make completion depend on the working directory rather than only on the source, so it is recorded as a follow-up instead.
