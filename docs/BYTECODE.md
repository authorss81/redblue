# The Redblue bytecode format (`.rbc`)

**Format version: 4.** This document is the normative description of the file
`rb compile` writes and `rb dis` reads. The encoder and decoder that implement
it are `src/bytecode/format.rs`; the instruction set is
`src/bytecode/opcode.rs`.

Stage **S1a** defines the format and the two tools. Nothing runs a `.rbc` yet
— that is stage S1b — so this is a description of a file, not of an
interpreter.

## Reading this document

Every number in a byte offset column is a byte count. Multi-byte integers are
**little-endian, unsigned, two's complement-free**: `u16` for the version,
`u32` for everything else. There is no padding anywhere and no alignment
requirement: a `.rbc` is a packed byte stream and is read in exactly the order
it is written.

Two rules govern every change to this format:

1. **A byte value already written into a file never changes.** An opcode, a
   block kind and a constant tag keep their numbers forever. A retired one is
   reserved, not reused.
2. **Any change that would make an older file decode differently bumps
   `FORMAT_VERSION`.** A reader refuses a version it does not know rather than
   guessing.

## File layout

```
+----------------------------------+
| magic          4 bytes  "RED\x1a"|
+----------------------------------+
| format version 2 bytes  u16      |
+----------------------------------+
| constant count  4 bytes  u32      |
+----------------------------------+
| constant pool   variable          |
+----------------------------------+
| main block      variable          |
+----------------------------------+
```

### Magic

`52 45 44 1a` — `RED` followed by `0x1A`, the "end of text" control character.
The control byte means a file whose line endings some tool converted is
detected rather than half-read.

A file that does not begin with the magic is refused with
`not a Redblue bytecode file`.

### Format version

A `u16`. This build reads and writes `4`. Any other value is refused with
`unknown bytecode format version <n>: this build reads version 4`.

#### What each version changed

Versions 1 and 2 changed no byte value and no record layout — only what an
operand means, and one refusal. Versions 3 and 4 each added instructions, at
byte values no earlier version used, so the numbers written into a file still
mean what they meant.

| Version | Change |
|---|---|
| 1 | The original format. |
| 2 | `DEF_OBJECT`'s secondary operand became the constant index of the object it extends — `NO_CONST` when it extends nothing — where version 1 wrote a flag that discarded the parent's name. `DEF_FIELD` became the consumer of the value pushed immediately before it, so a field's `default` is compiled instead of dropped. The decoder refuses a constant pool that reaches the reserved index `NO_CONST`. |
| 3 | Added `DECLARE_CONST`, which `constant NAME to <expr>` compiles to. Versions 1 and 2 compiled the same declaration to `STORE`, which does not say the name is read-only, so the file could not be told apart from a `set`. A version-2 file is refused rather than read as a program whose constants could be rebound. |
| 4 | `IMPORT`'s secondary operand became the name it binds — the alias the source wrote, or the module's own name when it wrote none — where version 3 wrote the filler `0`. Read as version 4, a version-3 file's `IMPORT` would bind `constants[0]`, which is whichever name happened to be first in the pool rather than the one the source named. Added `MODULE` and `EXPORT`, which a `module NAME … end` declaration compiles to; version 3 compiled such a declaration's body inline and the declaration itself to nothing, so a file written then says nothing about where the module begins or what it publishes. A version-3 file is refused rather than half-read. |

An earlier file is refused rather than half-read: the same bytes would mean
two different things, and rule 2 above is what makes that a new version rather
than an edit.

### Constant pool

The pool is **one per file**, shared by every block, so a name used inside a
function body is the same entry as the same name used outside it.

Each entry starts with a one-byte tag:

| Tag | Constant | Payload |
|---|---|---|
| 0 | `nothing` | — |
| 1 | number | 8 bytes, IEEE-754 `f64` |
| 2 | `yes` / `no` | 1 byte: `0` is no, `1` is yes, anything else is refused |
| 3 | text | `u32` byte length, then that many bytes of UTF-8 |

Text that is not valid UTF-8 is refused. A length longer than the bytes left in
the file is refused before anything is allocated.

Text entries are interned by the compiler: a name used a hundred times is one
entry. Number and `yes`/`no` entries are not interned.

The index `NO_CONST` (`0xFFFFFFFF`) is reserved: it is the operand that says
"this instruction names no constant", and a file whose declared count would make
it a real index is refused, so the operand is never ambiguous.

### Blocks

A block is one `to … end`, one `object … end`, one `test "…" … end`, one
`module … end`, one handler of a `try`, or the program's own `main`. It is the
unit a jump target is resolved against: **an instruction may only jump to an
instruction of its own block.**

```
kind            1 byte
arity           4 bytes  u32
name length     4 bytes  u32
name            that many bytes of UTF-8
instruction count 4 bytes u32
instructions    instruction count * 13 bytes
child block count  4 bytes u32
child blocks    variable
```

| Kind byte | Block |
|---|---|
| 0 | `main` |
| 1 | function |
| 2 | method |
| 3 | object |
| 4 | test |
| 5 | catch body |
| 6 | finally body |
| 7 | module body |

`arity` is the parameter count of a function, method or test block and `0`
for everything else. `name` is the declaration's name; for a catch body it is
the name the `catch` bound, which is empty when `catch` bound nothing. Child
blocks are in the order the compiler emitted them, and an instruction refers to
one by its **index in that list**, so the order is part of the format.

Blocks nest at most `MAX_BLOCK_DEPTH` (64) deep, the same budget the parser
enforces on `to … end` nesting. A file that nests deeper is refused.

The limit bounds what a *file* may hold, not what the in-memory tree may be:
`Chunk`'s fields are public, so a caller can build a deeper tree than the
compiler emits. Every walk over one — `Chunk::encode`, `Chunk::all_blocks` and
`rb dis` — keeps its pending blocks on the heap, and dropping a `Block` does
the same, so a tree of any depth is encoded, printed and freed rather than
overflowing the stack. A tree past the limit is simply a file this build's
decoder refuses.

### Instructions

Every instruction is exactly **13 bytes**.

```
opcode     1 byte
arg        4 bytes  u32   primary operand
aux        4 bytes  u32   secondary operand
line       4 bytes  u32   1-based source line, 0 when synthetic
```

`arg` is a constant index, a block index, a jump target or an element count,
depending on the opcode. `aux` is a second operand where the opcode needs one.
An opcode that uses neither leaves both at `0`; the encoders never leave a
meaningful value in an unused field.

`line` is the line of the statement the instruction came from, for a reader
stepping through a `.rbc` next to the source it came from. It is not used for
anything else.

## The instruction set

`arg` and `aux` columns say what the two operands mean; `—` means the field is
always `0`.

| Byte | Name | `arg` | `aux` | Stack effect |
|---|---|---|---|---|
| 0 | `NOP` | `END_TRY_MARKER`, or `0` for the filler | — | nothing; see below |
| 1 | `PUSH_CONST` | constant index | — | pushes the constant |
| 2 | `POP` | — | — | drops the top |
| 3 | `LOAD` | name index | — | pushes the value of a variable |
| 4 | `STORE` | name index | — | pops into a variable |
| 5 | `SAY` | — | — | prints and pops |
| 6 | `PRINT` | — | — | prints in internal form and pops |
| 7 | `LOAD_PROPERTY` | field name index | — | pops an object, pushes a field |
| 8 | `SET_PROPERTY` | field name index | — | pops a value and an object, sets the field |
| 9 | `INDEX` | — | — | pops an index and a list, pushes the element |
| 10 | `BUILD_LIST` | element count | — | pops that many values, pushes a list |
| 11 | `BUILD_RECORD` | pair count | — | pops that many key/value pairs, pushes a record |
| 12 | `BUILD_TEXT` | part count | — | pops that many values, pushes their concatenation |
| 13 | `ADD` | — | — | pops two, pushes the sum |
| 14 | `SUB` | — | — | pops two, pushes the difference |
| 15 | `MUL` | — | — | pops two, pushes the product |
| 16 | `DIV` | — | — | pops two, pushes the quotient |
| 17 | `MOD` | — | — | pops two, pushes the remainder |
| 18 | `EQUAL` | — | — | pops two, pushes equality |
| 19 | `NOT_EQUAL` | — | — | pops two, pushes inequality |
| 20 | `LESS` | — | — | pops two, pushes `<` |
| 21 | `LESS_EQUAL` | — | — | pops two, pushes `<=` |
| 22 | `GREATER` | — | — | pops two, pushes `>` |
| 23 | `GREATER_EQUAL` | — | — | pops two, pushes `>=` |
| 24 | `AND` | — | — | pops two, pushes conjunction |
| 25 | `OR` | — | — | pops two, pushes disjunction |
| 26 | `IN` | — | — | pops two, pushes membership |
| 27 | `NEG` | — | — | pops one, pushes its negation |
| 28 | `NOT` | — | — | pops one, pushes its negation |
| 29 | `CALL` | name index | argument count | pops the arguments, pushes the result |
| 30 | `CALL_METHOD` | name index | argument count | pops the arguments and a receiver, pushes the result |
| 31 | `RETURN` | — | — | pops the returned value, or `nothing` |
| 32 | `BREAK` | — | — | leaves the innermost loop |
| 33 | `SKIP` | — | — | starts the next turn of the innermost loop |
| 34 | `JUMP` | target | — | continues at `target` in this block |
| 35 | `JUMP_IF_FALSE` | target | — | pops a value; continues at `target` when it is not truthy |
| 36 | `GET_ITER` | — | — | turns the top into an iterator |
| 37 | `GET_RANGE` | — | operand count | builds a range: 1 count, or 2 bounds, or 2 bounds and a step |
| 38 | `DEF_FUNCTION` | block index | parameter count | binds a closure whose body is that block |
| 39 | `DEF_METHOD` | block index | parameter count | adds a method to the object being defined |
| 40 | `DEF_OBJECT` | block index | parent name index, or `NO_CONST` | starts an object body; the block holds its fields and methods |
| 41 | `DEF_FIELD` | field name index | — | pops the field's initial value and declares it on the object being defined |
| 42 | `TRY` | catch block index, or `NO_BLOCK` | finally block index, or `NO_BLOCK` | runs what follows with those handlers in place |
| 43 | `IMPORT` | module name index | alias name index | imports a module, binding it the alias; the module's own name when the source wrote none |
| 44 | `TEST` | block index | — | runs the test body |
| 45 | `EXPECT` | — | — | pops the expected and the actual value and compares them |
| 46 | `DECLARE_CONST` | name index | — | pops into a variable as a constant: the name may not be bound again, and no `STORE` writes it |
| 47 | `MODULE` | module body block index | — | starts a module declaration; the block holds what it publishes and its body, and its name is the module's |
| 48 | `EXPORT` | published name index | `NO_CONST` when the declaration does not define that name | nothing: data the `MODULE` that entered the block has already read |

`NO_BLOCK` is `0xFFFFFFFF`, the operand that says "this handler is not there".
A `try` with no `catch` and no `finally` writes it in both fields and creates
no blocks.

`NO_CONST` is `0xFFFFFFFF` too, and says "this instruction names no constant":
`DEF_OBJECT` writes it for an object that extends nothing, and `EXPORT` writes it
in its second operand for a name the declaration does not define — which a VM
refuses rather than publishes. The two reserved values are never compared against
each other — `NO_BLOCK` appears only in a block-index operand and `NO_CONST` only
in a constant-index one.

`END_TRY_MARKER` is `0xFFFFFFFF` as well, and is the one value of a `NOP`'s
`arg` that is not the filler: it is the end of a `try`'s protected region,
where the handlers are popped and the `finally` runs. Every other `arg` — the
`0` a filler carries — leaves the instruction doing nothing. A `NOP`'s `arg`
indexes nothing at all, so this third user of the reserved value is in neither of
the two operand kinds the sentence above compares.

The compiler writes exactly one `NOP` per `try`, immediately after that `try`'s
protected code, carrying `END_TRY_MARKER`. Without it a failure in a statement
*after* the `try` would be caught by that `try`, and a `finally` would never run
at all on the path where nothing failed. A `NOP` is one byte with two meanings
because the table was frozen when the marker was defined: byte 46 was the last
instruction, so an end-of-try byte of its own would have had to renumber an
instruction whose byte value is part of the format. The operand is what tells the
two apart, which is why a
filler `NOP` anywhere — including inside protected code — closes nothing, runs
no `finally`, and does not shorten the region a failure skips. `rb dis` prints
`end of a protected region` on a marked one.

An opcode byte this table does not assign is refused with
`unknown opcode byte <n>`.

### Stack discipline

A block is entered with an empty stack. An expression leaves its value on top,
and a statement leaves nothing behind — `STORE`, `SET_PROPERTY`, `POP`, `SAY`
and `PRINT` all consume what their statement produced.

`DEF_FIELD` is a statement that consumes one: the code in front of it pushes
the field's initial value, which is what makes a field's `default` able to be
any expression rather than only a literal. A `has` with no `default` pushes
`nothing` first, which is the value the tree-walking VM gives such a field.

Two things are held across a stretch of instructions rather than consumed at
once:

- A loop holds its iterator (`for each`) or its range (`for each … from`, or
  `repeat … times`) on the stack for the whole body. No instruction pops it;
  stage S1b decides where it is released.
- `TRY` names handler blocks rather than jumping to them. When the protected
  code faults, the failed value is the value those handlers are entered with.
  Stage S1b defines how a `catch` block reads it and how a `finally` block runs
  whether or not there was a fault.

A jump target is an instruction offset in the block that holds it, or the
block's instruction count — one past the last instruction — which means "leave
this block". That second form is not an exception: a `while` loop or a bare
`if` at the end of a block has no instruction after it, so its exit jump has
nowhere else to point. A target *past* the instruction count is malformed, and
`rb dis` reports it.

### Names, not slots

Names are stored as text, not as slot numbers. A Redblue function closes over
the local scopes that were live where it was declared, so the name has to
survive into the file for a VM to resolve it against those scopes. Resolving
names to slots is a later stage, and a version bump.

The one name the compiler introduces is `$counter`, the counter of a
`repeat … times` loop. `$` is not an identifier character in Redblue, so it
cannot collide with a name the program declared.

## How each statement compiles

`say` and `print` push their expression and then `SAY` or `PRINT`. `set x to e`
compiles the expression then `STORE x`. `constant x to e` compiles the
expression then `DECLARE_CONST x`, which is `STORE x` with the declaration that
the name is read-only: a VM that runs this file refuses a second `DECLARE_CONST`
of the name and a `STORE` that names it, which is what the tree-walking
interpreter refuses. `if` compiles the condition, a `JUMP_IF_FALSE`, the
then-branch, and — only when there is an `else` — a `JUMP` over it.

`for each x in e` compiles `e`, `GET_ITER`, then the loop: `STORE x`, the
body, `JUMP` back to the `STORE`. `for each x from a to b` and
`for each x from a to b by s` compile their bounds, `GET_RANGE` with `aux` 2 or
3, then the same loop shape. `repeat n times` is `GET_RANGE` with `aux` 1.
`while` puts its condition at the top, jumps out on false, and jumps back at
the end.

`break` and `skip` are opcodes rather than jumps, so a loop's shape does not
have to be rewritten to find them.

`to f(a)` compiles `DEF_FUNCTION` naming a function block, then `STORE f`.
`object Name … end` compiles `DEF_OBJECT` naming an object block, then
`STORE Name`; the object body holds `DEF_FIELD` for each `has` and
`DEF_METHOD` for each `to can`, and each method is a block of the object body.
`object Child extends Parent … end` is the same shape with the parent's name
in `DEF_OBJECT`'s secondary operand, interned like any other name, so the file
says *which* object is extended rather than only that one is. `test "name"`
compiles `TEST` naming a test block.

`has f default e` compiles `e` and then `DEF_FIELD f`, which pops the value the
expression produced; `has f` compiles a push of `nothing` and then `DEF_FIELD f`.
Either way the object body reads as one initialisation per field, in source
order.

`try … end` compiles `TRY` naming a catch block and a finally block, then the
protected statements inline. The handlers are separate blocks so the protected
code stays a straight run of instructions with no jump patching.

`module Name … end` compiles `MODULE` naming the declaration's body block, whose
name is the module's. The
body is a block of its own, so it runs in a scope and a frame of its own: a `set`
inside a module is the module's name and not a name of the program that declared
it. The block opens with one `EXPORT` per name the declaration publishes, and the
rest of the block is the body. The `EXPORT`s come first so a jump inside the body
still points at the instruction it was compiled for — nothing is inserted after
the body was compiled. `export all` is written as one `EXPORT` per declared name
rather than as a flag, so the file says what the declaration publishes without
the reader having to know the rule; an `export` of a name the module does not
define carries `NO_CONST`, which is a VM's cue to refuse it rather than publish
it, in the same words the tree-walking interpreter uses.

`import Name as Alias` compiles `IMPORT` naming the module and the alias, then
`STORE Alias`. A VM that finds the alias already bound steps over the `STORE`
rather than running it: an import does not take over a name the program holds.

## Reading a file that is not one of ours

A decoder must refuse, not guess:

- a file shorter than the magic, or with the wrong magic
- a version it does not know
- a file that ends in the middle of any field
- a count — constants, instructions, blocks, a string length — larger than the
  bytes actually left, **checked before anything is allocated**
- a constant count that reaches the reserved index `NO_CONST`, checked from the
  declared count before any entry is read
- a constant tag, `yes`/`no` byte or block kind this build does not assign
- a string that is not valid UTF-8
- a block nested deeper than `MAX_BLOCK_DEPTH`
- any bytes after the last block

Every one of these is an `Error::Parser` with `Span::unknown()` and a message
naming what was wrong.

An *operand* is a different matter. The decoder reads records; it does not
interpret what they address, so a file whose constant index, block index, parent
name or jump target is out of range is structurally sound and is decoded. Those
values are reported by `rb dis`, which is where a person reads them, and any VM
has to check them too when it resolves them. What the decoder does refuse is a
structure it cannot hold: a tag it does not assign, a count past the bytes
left, a depth past the limit.

## What `rb dis` prints

```
; redblue bytecode v4
; constants: 2
;   [0] 1
;   [1] n

main (main, arity 0)
0000  PUSH_CONST       0  ; line 1: 1
0001  STORE            1  ; line 1: n
0002  LOAD             1  ; line 2: n
0003  SAY                ; line 2
```

(from `rb compile` / `rb dis` on `set n to 1` + `say n`)

The header states the format version and the whole constant pool, so a reader
never has to count down to find out which name an index holds. Each block is
headed by its path (`main`, `main.blocks[0]`, `main.blocks[0].blocks[1]`), its
kind and its arity, and its instructions are indented one level per level of
nesting. The mnemonic column is padded to the width of the longest mnemonic in
the *whole* table, so it does not shift with the program.

Each instruction line is the offset, the mnemonic, the operands, and a comment
carrying the source line and what each operand names. **Every index it prints
is checked** against the thing it indexes, and an index that is out of range is
reported in that comment rather than read:

- a constant index outside the pool, including the parent of a `DEF_OBJECT`
- a block index outside the block's own list, for `DEF_FUNCTION`, `DEF_METHOD`,
  `DEF_OBJECT`, `MODULE`, `TEST` and both halves of a `TRY`
- a jump target past the end of its block; a jump to the block's instruction
  count is reported as `end of block`, because that is the documented exit

An operand that has no value to name says nothing: `DEF_OBJECT`'s `NO_CONST`
prints as `none` rather than as a four-billion-and-something index, an `EXPORT`'s
`NO_CONST` prints as `not defined` rather than as one, and a `TRY`'s absent
handler prints nothing at all. Where an instruction names more than one thing,
the clauses are separated by `;`.

The disassembler is a pure function of the chunk: it walks blocks and
instructions in index order and never sorts or hashes, so `rb dis` prints the
same bytes for the same file on every run. It keeps its pending blocks on the
heap rather than recursing per nesting level, so a `Chunk` far deeper than
`MAX_BLOCK_DEPTH` — which the public fields let a caller build — is printed in
full instead of overflowing the stack.
