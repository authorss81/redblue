# Phase 100 — Path values and directory listing

## What changed

The finding reproduced on the phase base before any edit: `path` was not a
module, so `say path.join("a", "b")` failed with `AnalyzerError: Unknown
variable 'path'`. Every program that built a filesystem path had to concatenate
`/` itself, which is wrong on Windows and has no answer for a parent, a filename,
an extension or the contents of a directory.

A path is now a `path` object with one field, `text`. It is deliberately not a
new `Value` variant: it prints as its text, compares equal to that text, and
`type_of` answers `path`. `path.join`, `path.parent` and `path.resolve` answer
path values; `path.filename`, `path.stem` and `path.extension` answer text or
`nothing`; `path.is_absolute` answers yes/no; and `path.list` answers the sorted
names directly inside a directory. The separators come from
`std::path::MAIN_SEPARATOR`, not a hardcoded slash.

`files.read`, `write`, `append`, `exists`, `lines`, `delete`, `copy` and
`rename` accept either a text path or a path value. `bytes.write` now accepts the
same two forms, so a path value can be used for every filesystem write rather
than only text writes. The old text spelling keeps working, and the old
argument-error phrases remain visible in the refusal text. Both engines reach the
same implementation through `runtime::builtin`.

The in-memory wasm filesystem answers `read_dir` as well, so `path.list` is not a
native-only surface. A directory listing is sorted by name and an empty
directory is the empty list. `.` and `./` name the current directory, so a file
written as `"a.txt"` is visible to the `path.list(".")` spelling SPEC.md
documents, and an empty current directory is the empty list rather than an
error. The flat in-memory filesystem still cannot distinguish a named empty
directory from a missing one; it reports `NotFound` for a named path with no
contents, the same answer a native `read_dir` gives for a missing directory.
`path.join` also keeps the drive or UNC prefix when a later part is rooted but
has no prefix of its own, matching `std::path::PathBuf::push`. The in-memory
filesystem's missing-path errors carry no path in their own text, because the
caller prefixes it once; the same sentence therefore reads the same in both
builds.

`path.resolve` needs a working directory, which `wasm32-unknown-unknown` does
not have. The playground refuses it with an `IoError` that names the platform
rather than answering a path that depends on a host it cannot see; the native
build is unchanged.

This run is the resumed continuation of that work. It finishes the path-value
routing for `bytes.write`, adds the missing integration coverage for
`append`/`delete`/`copy`/`rename` with path values, and records the phase-100
delivery commit in the unreviewed-work acknowledgement rule.

The final resume closes the last Windows join rule and restores the config
contract. A part that carries a drive but no root, such as `D:other`, now
replaces the whole path before it, matching `std::path::PathBuf::push`; before
this change it was appended as if it were an ordinary relative part. A
top-level `provider` block merged by a config commit broke 18 pre-existing
`agent_permissions_test` checks, so this run removes it; the project config
declares only `$schema` and `agent`, as its own contract requires. The two new
integration tests exercise `path.list` with a path value (sorted, empty, spaced
and non-ASCII names) and its refusal to list a file.

The review round pins two playground promises that were only implicit. The
`path.resolve` refusal is now a helper the host build also compiles under
`test`, so its `Unsupported` kind and platform-naming message are asserted by
`cargo test` instead of living in a wasm-only arm nobody compiles. The
in-memory listing splits only on `/`, because that is the playground's
separator: a backslash stays an ordinary filename character, so a file written
as `a\b.txt` is listed by that whole name rather than truncated to `a`.

The second review round closes three path edge cases. The in-memory directory
prefix now drops `.` segments, so `path.list("./data")` lists the file
`files.write("data/x.txt", …)` stored, instead of asking for a literal `./data/`
prefix no key carries. `path.join` keeps the separator on a UNC share root such
as `\\server\share\`, because trimming it leaves a prefix with no root. And
`path.resolve` refuses a Windows prefix without a root, such as `C:foo`, rather
than joining it to the working directory and answering a path that is still
relative.

The resumed continuation closes two remaining path-parity gaps. The in-memory
filesystem used to key files by their literal text, so `data/./x.txt` and
`data/sub/../x.txt` named nothing even though `data/x.txt` was stored and a
native `std::fs` call opens every spelling. It now folds `.` and `..` before
every lookup, write, append, copy, rename, remove and listing, and refuses a
directory spelling as a file with `IsADirectory`. A leading `..` on a relative
path has no parent to fold into in a flat map, so it names nothing: it is not a
literal key a listing can report, and a write through it is refused. Separately,
`path.resolve` on Windows now accepts a rooted path whose separator is `/`
(`C:/folder`), instead of mistaking it for a drive-relative path and refusing
it; the root may be spelled with either separator, as Windows itself allows.

The third review round closes three path edge cases. `file_key` now refuses a
spelling such as `data/.` or `data/sub/..` when it normalizes onto a directory
implied by the files stored beneath it, so it is `IsADirectory` instead of a
file that hides the directory. A forward-slash UNC spelling such as
`//server/share` is recognized as a prefix without a root, so Windows `resolve`
refuses it rather than joining it to the working directory. And a leading `..`
on a relative path now normalizes to nothing instead of a literal `..` key, so
it cannot be written or shown in `path.list(".")`.

| File | Lines | What |
|---|---|---|
| `src/path.rs` | +344 −0 | path value, `text_of`, `join`, `join_with_separator`, `parent`, `filename`, `stem`, `extension`, `is_absolute`, lexical `resolve`, `list`, the eight `path.*` builtin arms, and three unit tests |
| `src/runtime.rs` | +52 −25 | `path_*` dispatch, path-object property access and hashing, `files.*` accepting text or path values, and `bytes.write` accepting a path value |
| `src/vfs.rs` | +63 −4 | native and in-memory `read_dir`, with sorted names, an empty native directory answered as an empty list, and only `/` separating names in the playground |
| `src/value.rs` | +17 −0 | path values display as their text, answer `path` from `type_name`, and compare equal to that text |
| `src/stdlib.rs` | +60 −1 | registers the `path` module and its members, and corrects the `bytes.write` signature text |
| `src/lib.rs` | +1 −0 | exports `pub mod path` |
| `tests/stdlib_modules_test.rs` | +296 −2 | 15 path integration tests running both engines and comparing their answers; the final resume adds `edge_listing_accepts_a_path_value` and `edge_listing_a_file_path_value_is_an_io_error` |
| `src/path.rs` | +16 −0 | the final resume makes a Windows drive-relative part replace the path before it, matching `PathBuf::push`, with `edge_a_drive_relative_part_replaces_the_path_before_it` |
| `src/path.rs` | +17 −0 | the review round lifts the wasm `path.resolve` refusal into a helper the host suite asserts, with `edge_the_playground_refuses_resolve_without_a_working_directory` |
| `opencode.json` | −16 | removes the top-level `provider` block a config commit merged; the agent-permission contract allows only `$schema` and `agent` |
| `tests/stdlib_module_docs_test.rs` | +52 −0 | one answerable row per new builtin |
| `tests/stdlib_module_names_test.rs` | +5 −0 | the `path` module row |
| `tests/wasm_test.rs` | +70 −0 | in-memory directory listing tests, including the review round's backslash-is-a-filename-character case |
| `src/path.rs`, `src/vfs.rs`, `tests/wasm_test.rs` | +70 −8 | the second review round normalizes dot-relative directory prefixes, keeps a UNC root separator, and refuses a drive-relative resolve, with the three tests below |
| `src/vfs.rs` | +89 −48 | the resumed continuation folds `.` and `..` before every in-memory file lookup and listing, refuses directory spellings as files, and keeps the flat map's keys canonical |
| `src/path.rs` | +65 −2 | the resumed continuation accepts a forward-slash Windows drive root in `resolve` and pins both root spellings with unit tests |
| `tests/wasm_test.rs` | +146 −0 | four in-memory tests for dotted file spellings, parent segments in listings, refusal of a file as a directory, and spaced/non-ASCII names |
| `tests/unreviewed_work_ack_test.rs` | +2 −1 | acknowledges the phase-100 delivery subject by rule and keeps the "another phase is not acknowledged" fixture meaningful |
| `tests/doc_blocks_runs.txt` | +9 −9 | re-anchors the SPEC blocks after the expanded files/path block |
| `tests/doc_blocks_fragments.txt` | +1 −1 | re-anchors the later network fragment |
| `tests/doc_blocks_known_fail.txt` | +1 −1 | re-anchors the later parser-failure block |
| `SPEC.md` | +32 −2 | documents the `path` surface, that `bytes.write` accepts a path value, and the Windows drive-relative join rule |
| `README.md` | +15 −1 | the user-facing files and path section |
| `SURVEY.md` | +7 −6 | builtin count 105 → 113, modules 13 → 14, members 76 → 84, the `path` row, and re-anchored citations |

A path value is `Value::Object("path", {text: ...})`. `path::text_of` is the one
conversion every path argument, every `files.*` call and `bytes.write` goes
through, so the text form and the value form cannot drift apart. `path.join`
follows the host rule that an absolute later part discards everything before it;
it also removes a trailing separator and folds `/` to `\` when the test asks for
the Windows separator. `path.resolve` is lexical: it makes the path absolute
against the working directory and folds `.` and `..` without consulting the
filesystem or resolving symlinks.

## Tests added

| Test | Edge class covered |
|---|---|
| `path_join_uses_the_platform_separator` | platform separator: the language's join takes its separator from `MAIN_SEPARATOR` |
| `edge_the_separator_is_the_platforms_not_a_hardcoded_slash` | platform separator: POSIX and Windows joins are distinct, and the language agrees with the host |
| `path_components_are_literal_for_relative_and_absolute_inputs` | relative and absolute parents, filenames, stems, extensions and `is_absolute` |
| `edge_parent_of_a_root_is_nothing_and_a_trailing_separator_is_ignored` | empty/boundary: a root has no parent; a trailing separator does not make a filename empty |
| `edge_join_of_an_absolute_part_discards_what_came_before` | boundary: an absolute component replaces the relative prefix |
| `edge_join_keeps_the_drive_for_a_root_relative_part_on_windows` | boundary (Windows): a rooted part without a drive keeps the drive of the path before it |
| `edge_trailing_double_dot_and_spaced_paths_are_handled` | malformed_input/unicode: double separators, trailing separators, dot segments, spaces and non-ASCII names |
| `files_accept_a_path_value_as_well_as_text` | type_mismatch: `files.*` accept both the text spelling and the path-value spelling |
| `files_append_delete_copy_and_rename_accept_path_values` | type_mismatch: the remaining filesystem functions accept path values on both engines |
| `edge_join_accepts_a_path_value_as_a_part` | boundary: a path value is also an argument to a later join |
| `edge_bytes_write_accepts_a_path_value` | type_mismatch: `bytes.write` accepts the path value the module answers |
| `edge_path_join_with_no_parts_is_a_clean_failure` | malformed_input: an empty join is a catchable runtime error |
| `edge_listing_a_file_is_an_io_error_not_a_list` | malformed_input: a file is not a directory and is refused rather than listed |
| `edge_listing_is_sorted_and_an_empty_directory_is_an_empty_list` | empty and ordering: sorted names, an empty directory answers `[]` |
| `edge_bad_path_arguments_are_clean_runtime_failures` | type_mismatch and malformed_input: non-path arguments are catchable runtime errors; a missing directory is a catchable I/O error |
| `edge_empty_text_has_no_components` | empty: an empty path has no parent, filename, stem or extension |
| `path::tests::edge_join_with_each_separator_is_distinct` | unit: `/` and `\` produce different joins |
| `path::tests::edge_joining_a_drive_root_keeps_it_absolute` | unit: a drive root keeps its separator and a root is absolute |
| `path::tests::edge_join_keeps_the_drive_for_a_root_relative_part` | unit: a root-relative part keeps a drive or UNC prefix, and replaces the rest when there is none to keep |
| `path::tests::edge_resolve_removes_dot_segments` | unit: resolution removes `.` and `..` segments with the platform's separator |
| `edge_the_in_memory_filesystem_lists_the_current_directory` | boundary: `.` and `./` list the files written by their bare names |
| `edge_the_in_memory_filesystem_lists_an_empty_current_directory` | empty: an empty current directory exists and lists as `[]` under both `.` and `./` |
| `edge_an_object_named_path_with_an_extra_field_is_not_a_path_value` | type_mismatch: only an object with exactly one text field is a path value |
| `edge_listing_accepts_a_path_value` | unicode and ordering: a path value lists sorted spaced and non-ASCII names; an empty directory answers `[]` |
| `edge_listing_a_file_path_value_is_an_io_error` | malformed_input: a path value naming a file is a catchable `IoError`, not a listing |
| `path::tests::edge_a_drive_relative_part_replaces_the_path_before_it` | boundary (Windows): a part with a drive but no root replaces the path before it; on POSIX the colon is ordinary text |
| `path::tests::edge_the_playground_refuses_resolve_without_a_working_directory` | malformed_input/platform: the wasm `path.resolve` refusal is `Unsupported` and names the platform and the native alternative |
| `edge_the_in_memory_filesystem_treats_a_backslash_as_a_filename_character` | boundary (POSIX): on the playground only `/` separates components, so a backslash stays part of the filename |
| `edge_the_in_memory_filesystem_lists_a_dot_relative_directory` | boundary: `data/x.txt` is listed by `./data`, `././data`, `data/.` and `./data/` as well as `data`, and each spelling also exists |
| `path::tests::edge_joining_a_unc_share_root_keeps_it_absolute` | boundary (Windows): a UNC share root keeps the separator that makes it a root, and the joined result is absolute |
| `path::tests::edge_resolve_refuses_a_drive_relative_path` | boundary (Windows): `C:foo` has a prefix but no root and is refused with `InvalidInput` instead of answered as a relative path |
| `edge_the_in_memory_filesystem_folds_dot_and_parent_segments_for_files` | malformed_input/boundary: `data/./x.txt` and `data/sub/../x.txt` read, exist and delete the file `data/x.txt` stored |
| `edge_the_in_memory_filesystem_folds_parent_segments_in_a_directory_listing` | boundary: a trailing separator, a doubled separator, `.` and `..` all list the same directory |
| `edge_the_in_memory_filesystem_refuses_a_file_named_through_dot_segments_as_a_directory` | malformed_input: a dotted file spelling is `NotADirectory`, an empty or missing path is `NotFound`, and `data/.`, `data/sub/..` and `data/..` are refused as implied directories with `IsADirectory` |
| `edge_the_in_memory_filesystem_folds_segments_around_spaced_and_non_ascii_names` | unicode: spaces and non-ASCII names survive segment folding in reads and listings |
| `path::tests::edge_a_forward_slash_drive_root_is_not_a_prefix_without_a_root` | boundary (Windows): `C:/folder` and `C:\folder` are both rooted, so neither is refused |
| `path::tests::edge_a_drive_without_a_root_is_a_prefix_without_a_root` | boundary (Windows): `C:`, `C:folder` and `D:other` are refused, while an ordinary relative path is not |
| `path::tests::edge_a_unc_share_without_a_root_is_a_prefix_without_a_root` | boundary (Windows): a UNC share with no trailing separator is refused; a rooted UNC spelling is not |
| `path::tests::edge_resolve_accepts_a_forward_slash_drive_root` | boundary (Windows, cfg-gated): `C:/Windows/..` resolves to the drive root rather than being refused |
| `edge_the_in_memory_filesystem_refuses_a_relative_path_above_the_root` | malformed_input/boundary: a leading `..` names nothing, is not writable, and is not an entry in `path.list(".")` |

Mandatory matrix:

- empty — `edge_empty_text_has_no_components`, the empty directory in
  `edge_listing_is_sorted_and_an_empty_directory_is_an_empty_list`, and the
  root parent case.
- singleton — a single component is joined and listed unchanged; the one-file
  listing case is covered by the sorted-listing test.
- boundary — the root parent, a trailing separator, an absolute later part, a
  Windows drive-relative part, double separators, dot and parent segments, and
  a path-value join part are all pinned. The in-memory filesystem folds those
  segments before every lookup and listing, so a stored file has one name.
- out_of_bounds — N/A: `path` exposes no numeric index or argument order.
- type_mismatch — every `path.*` argument and every `files.*` path is checked;
  `edge_bad_path_arguments_are_clean_runtime_failures` and the path-value tests
  pin both the accepted and the refused forms.
- numeric_boundary — N/A: no numeric path operation is exposed.
- unicode — the spaced and non-ASCII filename and stem cases in
  `edge_trailing_double_dot_and_spaced_paths_are_handled`, and the in-memory
  read/listing case in
  `edge_the_in_memory_filesystem_folds_segments_around_spaced_and_non_ascii_names`.
- nesting_recursion — N/A: a path is flat text; no recursive user callback is
  invoked.
- duplicate_missing_keys — N/A: paths are not collections and do not have keys.
- malformed_input — missing directory, a file passed as a directory, a dotted
  spelling of a file asked to list, no join parts, non-text arguments and empty
  components are covered by the refusal and empty-path tests.
- resource_limit — N/A: no new numeric limit is introduced; a listing is
  bounded by the host directory.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 2425 passed, 0 failed, 0 ignored |
| `../rbops/verify.sh phase-100` | pass |

## Invariants touched

- None. `Value` has no new variant, `Error` has no new variant, and the grammar,
  `.rb` files, `to … end`, `set … to`, `say`, trailing commas, interpolation,
  examples and modules are unchanged.
- `path` is a module name; the new names are builtins reached by both engines
  through the existing shared dispatcher.
- A path value is an ordinary object with a `text` field, not a hidden value
  kind; it remains inspectable as a record.
- `bytes.write` now goes through the same `path::text_of` conversion as the
  filesystem functions; its accepted spellings gain a path value and its
  existing text form is unchanged.

## Known gaps / follow-ups

- `path.resolve` is lexical. It does not resolve symlinks or require the path to
  exist, which is the useful answer for a path that is about to be created; a
  canonicalising spelling would be a separate operation.
- `path.list` answers names only, not file type, size or modification time. A
  structured directory-entry value would be a follow-up.
- The in-memory wasm filesystem cannot distinguish a named empty directory from
  a missing one, so a named empty directory is reported as `NotFound` there; a
  flat path-to-bytes map has no way to remember a directory with no files under
  it. The working directory is the exception: `.` and `./` always exist and
  list as an empty list when nothing has been written.
- `path.resolve` is refused in the WebAssembly playground because the platform
  has no working directory to resolve against; the native build keeps the
  lexical behaviour above, and SPEC.md and README.md now say the same.
- A path value is a record-shaped object, so `json.stringify` uses its text
  spelling rather than a path-specific JSON form.
- The in-memory filesystem folds `.` and `..` lexically because a flat map has
  no directories to traverse. A leading `..` on a relative path therefore names
  nothing rather than the host's parent directory, so reading, listing or
  writing through it is `NotFound`; the native build resolves it against the
  real working directory.
