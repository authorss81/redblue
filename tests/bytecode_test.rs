//! Bytecode format, `rb compile`, `rb dis` (bootstrap stage S1a).
//!
//! These tests pin the format itself: that encoding is lossless and stable,
//! that decoding refuses a file it does not understand instead of guessing,
//! and that the disassembler is a deterministic function of the bytes.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use redblue::bytecode::{
    compile_source, disassemble, Block, BlockKind, Chunk, Constant, Instruction, Opcode,
    FORMAT_VERSION, MAGIC, NO_BLOCK, NO_CONST,
};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = project_root().join("target/tmp/bytecode-test").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

/// Every source file under `examples/`, sorted so the test itself is
/// deterministic.
fn example_sources() -> Vec<(String, String)> {
    let dir = project_root().join("examples");
    let mut found: Vec<(String, String)> = fs::read_dir(&dir)
        .expect("examples/ should be readable")
        .map(|entry| entry.expect("directory entry should be readable").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rb"))
        .map(|path| {
            let source = fs::read_to_string(&path).expect("example should be readable UTF-8");
            (
                path.file_name()
                    .expect("example has a name")
                    .to_string_lossy()
                    .to_string(),
                source,
            )
        })
        .collect();
    found.sort();
    assert!(!found.is_empty(), "examples/ has no .rb files to compile");
    found
}

fn ops(block: &Block) -> Vec<Opcode> {
    block.code.iter().map(|i| i.opcode).collect()
}

/// The opcode names of a block, in order — what a reader of the disassembly
/// sees.
fn names(block: &Block) -> Vec<&'static str> {
    block.code.iter().map(|i| i.opcode.name()).collect()
}

fn blocks_of_kind(block: &Block, kind: BlockKind) -> Vec<&Block> {
    let mut found: Vec<&Block> = block.blocks.iter().filter(|b| b.kind == kind).collect();
    for child in &block.blocks {
        found.extend(blocks_of_kind(child, kind));
    }
    found
}

fn walk(block: &Block, visit: &mut dyn FnMut(&Block)) {
    visit(block);
    for child in &block.blocks {
        walk(child, visit);
    }
}

// ---------------------------------------------------------------------------
// The format: encoding, decoding, versioning
// ---------------------------------------------------------------------------

#[test]
fn compile_encode_decode_round_trips_losslessly() {
    let chunk = compile_source(
        "set greeting to \"Hello\"\nsay greeting\nto shout(text)\n    say text\nend\nshout(greeting)\n",
    )
    .expect("hello program should compile");

    let bytes = chunk.encode();
    let decoded = Chunk::decode(&bytes).expect("a chunk this module wrote should decode");

    assert_eq!(
        decoded, chunk,
        "decoding the bytes of a compiled program did not give the program back"
    );
    assert_eq!(
        decoded.encode(),
        bytes,
        "encode(decode(bytes)) must reproduce the bytes, or the format is not stable"
    );
}

#[test]
fn every_example_compiles_and_encodes_deterministically() {
    for (name, source) in example_sources() {
        let chunk = compile_source(&source)
            .unwrap_or_else(|e| panic!("examples/{name} should compile: {e}"));

        assert_eq!(
            chunk.main.kind,
            BlockKind::Main,
            "examples/{name} should compile to a main block"
        );
        assert!(
            !chunk.main.code.is_empty(),
            "examples/{name} compiled to no instructions at all"
        );

        let first = chunk.encode();
        let second = compile_source(&source)
            .expect("second compile of the same source should succeed")
            .encode();
        assert_eq!(
            first, second,
            "examples/{name} encoded differently on two runs of the same compiler"
        );

        let decoded = Chunk::decode(&first)
            .unwrap_or_else(|e| panic!("examples/{name}.rbc should decode: {e}"));
        assert_eq!(
            decoded.encode(),
            first,
            "examples/{name} did not survive a decode/encode round trip"
        );
    }
}

#[test]
fn disassembly_is_a_deterministic_function_of_the_bytes() {
    let chunk = compile_source("say \"hi\"\n").expect("trivial program should compile");
    let bytes = chunk.encode();

    let first = disassemble(&chunk);
    let second = disassemble(&chunk);
    assert_eq!(first, second, "disassembling twice gave different text");

    let decoded = Chunk::decode(&bytes).expect("chunk should decode");
    assert_eq!(
        disassemble(&decoded),
        first,
        "the disassembly of the decoded bytes differs from the original"
    );
    assert!(
        first.contains("PUSH_CONST") && first.contains("SAY"),
        "disassembly omitted its opcodes:\n{first}"
    );
}

#[test]
fn edge_unknown_format_version_is_rejected() {
    let mut bytes = compile_source("say 1\n")
        .expect("program should compile")
        .encode();
    let future = (FORMAT_VERSION + 1).to_le_bytes();
    bytes[4..6].copy_from_slice(&future);

    let error = Chunk::decode(&bytes).expect_err("a future version must not be accepted");

    assert!(
        error.message().contains(&format!("{future:?}")) || error.message().contains("version"),
        "the error should name the version it cannot read, got: {error}"
    );
}

#[test]
fn edge_bad_magic_is_rejected() {
    let mut bytes = compile_source("say 1\n")
        .expect("program should compile")
        .encode();
    bytes[0] = b'X';

    let error = Chunk::decode(&bytes).expect_err("a file that is not bytecode must be refused");

    assert!(
        error.message().contains("bytecode"),
        "the error should say the file is not bytecode, got: {error}"
    );
}

#[test]
fn edge_every_truncation_of_a_valid_file_is_rejected_without_panicking() {
    let bytes = compile_source(
        "set list to [1, \"two\", yes]\nsay list\nfor each item in list\n    say item\nend\n",
    )
    .expect("program should compile")
    .encode();

    for cut in 0..bytes.len() {
        let error = Chunk::decode(&bytes[..cut])
            .expect_err("every prefix short of the whole file is malformed");
        assert!(
            error.message().contains("truncated")
                || error.message().contains("magic")
                || error.message().contains("bytecode"),
            "prefix of {cut} bytes gave an unhelpful error: {error}"
        );
    }
    assert!(
        Chunk::decode(&bytes).is_ok(),
        "the whole file must still decode after the prefixes were refused"
    );
}

#[test]
fn edge_unknown_opcode_byte_is_rejected() {
    let bytes = compile_source("say 1\n")
        .expect("program should compile")
        .encode();

    let bad = 0xFFu8;
    let mut corrupt = bytes.clone();
    corrupt[first_opcode_offset()] = bad;

    let error = Chunk::decode(&corrupt).expect_err("an unassigned opcode must be refused");
    assert!(
        error.message().contains("opcode") || error.message().contains("255"),
        "the error should name the unknown opcode, got: {error}"
    );
}

/// Byte offset of the first instruction record in an encoded chunk: header,
/// then the constant pool, then the main block header. Rather than recompute
/// it, the test rewrites the *first* opcode byte by searching for a record
/// whose other fields are known — so it locates the offset directly.
fn first_opcode_offset() -> usize {
    // The constant pool of `say 1` is a single number constant (tag + 8
    // bytes), so the layout is fixed and small enough to walk by hand. A block
    // header is kind, arity, parameter count, the empty parameter list, the
    // name length and the name `main`, then the instruction count.
    let header = 4 + 2 + 4;
    let constants = 1 + 8;
    let block_header = 1 + 4 + 4 + 4 + 4 + 4;
    header + constants + block_header
}

#[test]
fn edge_non_utf8_string_constant_is_rejected() {
    let chunk = compile_source("say \"hi\"\n").expect("program should compile");
    let bytes = chunk.encode();
    let text_at = bytes
        .windows(2)
        .position(|w| w == b"hi")
        .expect("the text constant should appear verbatim in the file");

    let mut corrupt = bytes.clone();
    corrupt[text_at] = 0xFF;

    let error = Chunk::decode(&corrupt).expect_err("invalid UTF-8 must be refused");
    assert!(
        error.message().contains("text") || error.message().contains("UTF-8"),
        "the error should say the constant is not text, got: {error}"
    );
}

#[test]
fn edge_declared_counts_larger_than_the_file_are_refused() {
    let bytes = compile_source("say 1\n")
        .expect("program should compile")
        .encode();

    // The constant count sits directly after magic and version. Claiming far
    // more constants than the file has bytes for must fail on the count, not by
    // trying to allocate. The count is below the reserved index `NO_CONST`, so
    // this is the bytes-left check; the reserved one is
    // `edge_a_pool_that_reaches_the_reserved_index_is_refused`.
    let too_many = 1_000_000u32;
    let mut lying = bytes.clone();
    lying[6..10].copy_from_slice(&too_many.to_le_bytes());

    let error = Chunk::decode(&lying).expect_err("a count past the end of the file is malformed");
    assert!(
        error.message().contains("constant"),
        "the error should name the constant count, got: {error}"
    );

    // Same for the instruction count of the main block.
    let instruction_count_at = first_opcode_offset() - 4;
    let mut lying = bytes.clone();
    lying[instruction_count_at..instruction_count_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let error = Chunk::decode(&lying).expect_err("an instruction count past the file is malformed");
    assert!(
        error.message().contains("instruction"),
        "the error should name the instruction count, got: {error}"
    );
}

/// `NO_CONST` is the operand that says "this instruction names no constant" —
/// `DEF_OBJECT` writes it for an object that extends nothing. A file claiming a
/// pool long enough to reach it would make the operand ambiguous, so it is
/// refused from the declared count, before anything is read or allocated.
#[test]
fn edge_a_pool_that_reaches_the_reserved_constant_index_is_refused() {
    let bytes = compile_source("say 1\n")
        .expect("program should compile")
        .encode();

    let mut lying = bytes.clone();
    lying[6..10].copy_from_slice(&NO_CONST.to_le_bytes());

    let error = Chunk::decode(&lying).expect_err("a pool reaching the reserved index is refused");

    assert!(
        error.message().contains("reserved") && error.message().contains(&NO_CONST.to_string()),
        "the refusal should name the reserved index, got: {error}"
    );
}

// ---------------------------------------------------------------------------
// Codegen
// ---------------------------------------------------------------------------

#[test]
fn expressions_compile_to_the_documented_instruction_sequences() {
    let chunk = compile_source(
        "set xs to [1, 2]\nsay xs[0]\nset rec to {a: 1}\nsay rec.a\nsay \"n is {1 + 2}\"\n",
    )
    .expect("program should compile");

    assert_eq!(
        names(&chunk.main),
        vec![
            "PUSH_CONST",
            "PUSH_CONST",
            "BUILD_LIST",
            "STORE",
            "LOAD",
            "PUSH_CONST",
            "INDEX",
            "SAY",
            "PUSH_CONST",
            "PUSH_CONST",
            "BUILD_RECORD",
            "STORE",
            "LOAD",
            "LOAD_PROPERTY",
            "SAY",
            "PUSH_CONST",
            "SAY",
        ],
        "expression codegen drifted from the documented shape"
    );

    assert_eq!(
        chunk.main.code[2].arg, 2,
        "BUILD_LIST carries its element count"
    );
    assert_eq!(chunk.main.code[6].arg, 0, "INDEX carries its index");
    assert_eq!(
        chunk.main.code[10].arg, 1,
        "BUILD_RECORD carries its pair count"
    );
    assert_eq!(
        chunk.constants[chunk.main.code[13].arg as usize],
        Constant::Text("a".to_string()),
        "LOAD_PROPERTY names the field through the constant pool"
    );
}

#[test]
fn calls_carry_their_name_and_arity_and_properties_carry_their_name() {
    let chunk = compile_source("say files.read(\"a\")\n").expect("program should compile");

    let call = chunk
        .main
        .code
        .iter()
        .find(|i| i.opcode == Opcode::CallMethod)
        .expect("files.read is a method call");

    assert_eq!(call.aux, 1, "CALL_METHOD carries its argument count");
    assert_eq!(
        chunk.constants[call.arg as usize],
        Constant::Text("files.read".to_string()),
        "CALL_METHOD names the receiver as well as the method, because a call is \
         resolved against the receiver's name"
    );

    let chunk = compile_source("set p to { name: 1 }\nset p.name to 2\n").expect("should compile");
    let set = chunk
        .main
        .code
        .iter()
        .find(|i| i.opcode == Opcode::SetProperty)
        .expect("an assignment to a field is a SET_PROPERTY");
    assert_eq!(
        chunk.constants[set.arg as usize],
        Constant::Text("p.name".to_string()),
        "SET_PROPERTY names the binding as well as the field, because the field is \
         written back to it"
    );
}

#[test]
fn loops_and_branches_produce_jumps_inside_their_own_block() {
    let chunk = compile_source(
        "for each item in [1, 2]\n    if item is 1 then\n        say item\n    end\nend\nwhile yes\n    skip\nend\n",
    )
    .expect("program should compile");

    let mut ops = names(&chunk.main);
    assert_eq!(
        ops,
        vec![
            "PUSH_CONST",
            "PUSH_CONST",
            "BUILD_LIST",
            "GET_ITER",
            "STORE",
            "LOAD",
            "PUSH_CONST",
            "EQUAL",
            "JUMP_IF_FALSE",
            "LOAD",
            "SAY",
            "JUMP",
            "PUSH_CONST",
            "JUMP_IF_FALSE",
            "SKIP",
            "JUMP",
        ],
        "loop and branch codegen drifted"
    );

    let backward: Vec<&Instruction> = chunk
        .main
        .code
        .iter()
        .enumerate()
        .filter(|(index, i)| {
            matches!(i.opcode, Opcode::Jump | Opcode::JumpIfFalse) && (i.arg as usize) <= *index
        })
        .map(|(_, i)| i)
        .collect();
    assert_eq!(
        backward.len(),
        2,
        "each loop needs exactly one backward jump, got {ops:?}"
    );
    ops.clear();
    assert!(
        ops.is_empty(),
        "the disassembly was consumed for the message"
    );
}

/// Every jump target is inside its own block *or* is that block's documented
/// exit — the instruction count, one past the last instruction. Both forms are
/// legal (`docs/BYTECODE.md` §"A jump target is…"); a target past the count is
/// not, and neither is a jump into another block.
#[test]
fn every_jump_target_is_inside_its_own_block() {
    let sources = [
        "set total to 0\nrepeat 3 times\n    set total to total + 1\n    if total is 2 then\n        skip\n    end\nend\n",
        // A loop and a bare `if` at the end of a block have no instruction
        // after them, so their exit jumps land on the instruction count. These
        // two programs are here so the `<=` below is not vacuously true: they
        // make the exit form the only legal target those jumps can have.
        "set i to 0\nwhile i is 1\n    say i\nend\n",
        "set i to 0\nif i is 1 then\n    say i\nend\n",
    ];

    let mut visited = 0usize;
    let mut backward = 0usize;
    let mut exits = 0usize;
    for source in sources {
        let chunk = compile_source(source)
            .unwrap_or_else(|e| panic!("program should compile: {e}\n{source}"));
        walk(&chunk.main, &mut |block| {
            visited += 1;
            for instruction in &block.code {
                if matches!(instruction.opcode, Opcode::Jump | Opcode::JumpIfFalse) {
                    assert!(
                        (instruction.arg as usize) <= block.code.len(),
                        "{:?} jumps to {} but block holds {} instructions",
                        instruction.opcode.name(),
                        instruction.arg,
                        block.code.len()
                    );
                    if instruction.arg as usize == block.code.len() {
                        exits += 1;
                    }
                }
                if instruction.opcode == Opcode::Jump {
                    backward += 1;
                }
            }
        });
    }
    assert!(visited >= 3, "the walk should have reached every block");
    assert_eq!(
        backward, 2,
        "`repeat` and `while` each compile to one loop-back jump, got {backward}"
    );
    assert_eq!(
        exits, 2,
        "the `while` and the `if` should each exit their block by jumping to the instruction count, got {exits}"
    );
}

/// A loop or a bare `if` at the end of a block has no instruction after it to
/// jump to, so its exit jump targets the block's instruction count. That is
/// the documented way out of a block, not a target past the end of one — and
/// `rb dis` must not report the compiler's own output as malformed.
#[test]
fn edge_a_jump_to_the_end_of_a_block_is_the_exit_not_an_out_of_range_target() {
    for (source, what) in [
        ("set i to 0\nwhile i is 1\n    say i\nend\n", "while"),
        ("set i to 0\nif i is 1 then\n    say i\nend\n", "if"),
    ] {
        let chunk = compile_source(source).unwrap_or_else(|e| panic!("{what} should compile: {e}"));
        let end = chunk.main.code.len() as u32;

        let targets: Vec<u32> = chunk
            .main
            .code
            .iter()
            .filter(|i| matches!(i.opcode, Opcode::Jump | Opcode::JumpIfFalse))
            .map(|i| i.arg)
            .collect();
        assert!(
            targets.contains(&end),
            "{what} at the end of a block should jump out to {end}, got {targets:?}"
        );
        for target in &targets {
            assert!(
                *target <= end,
                "{what} emitted a jump to {target}, past the end of {end} instructions"
            );
        }

        let text = disassemble(&chunk);
        assert!(
            !text.contains("outside this block"),
            "{what} produced a jump `rb dis` calls out of range:\n{text}"
        );
        assert!(
            text.contains("end of block"),
            "a jump out of the block should say where it lands:\n{text}"
        );

        let bytes = chunk.encode();
        assert_eq!(
            Chunk::decode(&bytes).expect("should round trip"),
            chunk,
            "{what}: the exit convention did not survive a round trip"
        );
    }
}

#[test]
fn edge_empty_program_compiles_to_an_empty_main_block() {
    let chunk = compile_source("").expect("an empty program should compile");

    assert!(chunk.main.code.is_empty(), "empty source emitted code");
    assert!(
        chunk.constants.is_empty(),
        "empty source allocated constants"
    );
    assert!(chunk.main.blocks.is_empty());

    let bytes = chunk.encode();
    assert_eq!(
        Chunk::decode(&bytes).expect("an empty chunk should round trip"),
        chunk
    );

    let text = disassemble(&chunk);
    assert!(
        text.contains(&format!("v{FORMAT_VERSION}")),
        "the disassembly should state the format version, got:\n{text}"
    );
}

#[test]
fn edge_a_single_statement_program_is_not_empty() {
    let chunk = compile_source("say \"only\"\n").expect("program should compile");

    assert_eq!(
        ops(&chunk.main).len(),
        2,
        "`say \"only\"` is PUSH_CONST, SAY"
    );
    assert_eq!(
        chunk.constants,
        vec![Constant::Text("only".to_string())],
        "one literal, one constant"
    );
}

#[test]
fn functions_tests_and_methods_become_named_blocks() {
    let chunk = compile_source(
        "to outer(a)\n    to inner(b)\n        give back b\n    end\nend\n\
         object Thing\n    has size\n    to can grow(by)\n        say by\n    end\nend\n\
         test \"thing grows\"\n    say 1\nend\n",
    )
    .expect("program should compile");

    let functions = blocks_of_kind(&chunk.main, BlockKind::Function);
    let found: Vec<&str> = functions.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(
        found,
        vec!["outer", "inner"],
        "nested declarations should nest as blocks, got {found:?}"
    );
    assert_eq!(functions[0].arity, 1);
    assert_eq!(functions[1].arity, 1);
    let outer = functions
        .iter()
        .find(|b| b.name == "outer")
        .expect("outer should be a block of main");
    assert_eq!(
        outer.blocks.len(),
        1,
        "the inner declaration should be a block of the outer one, got {:?}",
        outer.blocks
    );
    assert_eq!(outer.blocks[0].name, "inner");
    assert_eq!(outer.blocks[0].kind, BlockKind::Function);

    let objects = blocks_of_kind(&chunk.main, BlockKind::Object);
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "Thing");

    let methods = blocks_of_kind(&chunk.main, BlockKind::Method);
    assert_eq!(methods.len(), 1);
    assert_eq!(methods[0].name, "grow");
    assert_eq!(methods[0].arity, 1);
    assert_eq!(
        names(objects[0]),
        // A `has` with no `default` initialises its field to `nothing`, and
        // `DEF_FIELD` consumes that value — see
        // `a_field_default_is_compiled_before_the_declaration_that_consumes_it`.
        vec!["PUSH_CONST", "DEF_FIELD", "DEF_METHOD"],
        "an object body initialises its fields and declares its methods"
    );

    let tests = blocks_of_kind(&chunk.main, BlockKind::Test);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].name, "thing grows");
}

/// `object Child extends Parent` has to put `Parent` in the file. A flag
/// saying "this one extends something" is not enough: the parent chain is
/// walked by name (`src/vm.rs`, `declare_object`), and a file that carries only
/// the flag cannot be resolved by anything.
#[test]
fn an_extending_object_names_its_parent_in_the_constant_pool() {
    let chunk = compile_source(
        "object Base\n    has kind default \"base\"\nend\n\
         object Child extends Base\n    has size default 1\nend\n",
    )
    .expect("program should compile");

    let defs: Vec<&Instruction> = chunk
        .main
        .code
        .iter()
        .filter(|i| i.opcode == Opcode::DefObject)
        .collect();
    assert_eq!(defs.len(), 2, "each object declaration is one DEF_OBJECT");

    assert_eq!(
        defs[0].aux, NO_CONST,
        "an object that extends nothing writes the reserved index, not a 0"
    );
    assert_eq!(
        chunk.constants[defs[1].aux as usize],
        Constant::Text("Base".to_string()),
        "an extending object names its parent through the constant pool, got {:?}",
        chunk.constants
    );
    assert!(
        (chunk.constants.len() as u64) < NO_CONST as u64,
        "no pool this compiler writes can reach the reserved index"
    );
    assert_eq!(
        chunk
            .constants
            .iter()
            .filter(|c| **c == Constant::Text("Base".to_string()))
            .count(),
        1,
        "`Base` is interned once, so the parent and the parent's own binding are \
         the same constant: {:?}",
        chunk.constants
    );

    let bytes = chunk.encode();
    assert_eq!(
        Chunk::decode(&bytes).expect("should round trip"),
        chunk,
        "the parent name must survive a round trip"
    );

    let text = disassemble(&chunk);
    let lines: Vec<&str> = text.lines().filter(|l| l.contains("DEF_OBJECT")).collect();
    assert_eq!(lines.len(), 2, "one line per declaration:\n{text}");
    assert!(
        lines[0].contains(", none"),
        "an object that extends nothing should read `none`, not a reserved index:\n{text}"
    );
    assert!(
        lines[1].contains("extends Base"),
        "the disassembly should say which object is extended:\n{text}"
    );
}

/// A field's `default` is any expression, so it is compiled: the value is
/// pushed in front of the `DEF_FIELD` that consumes it. Dropping it, as an
/// earlier build did, lost `has size default 5 + 2` entirely — the file said
/// the field existed and nothing about what it was initialised to.
#[test]
fn a_field_default_is_compiled_before_the_declaration_that_consumes_it() {
    let chunk = compile_source("object Thing\n    has size default 5 + 2\n    has label\nend\n")
        .expect("program should compile");

    let objects = blocks_of_kind(&chunk.main, BlockKind::Object);
    assert_eq!(objects.len(), 1, "the object body is one block");
    let body = objects[0];

    assert_eq!(
        names(body),
        vec![
            "PUSH_CONST",
            "PUSH_CONST",
            "ADD",
            "DEF_FIELD", // size, defaulted to 5 + 2
            "PUSH_CONST",
            "DEF_FIELD", // label, with no default
        ],
        "a default is compiled in front of the declaration that consumes it"
    );

    assert_eq!(
        chunk.constants[body.code[0].arg as usize],
        Constant::Number(5.0),
        "the default's own constants are in the pool: {:?}",
        chunk.constants
    );
    assert_eq!(
        chunk.constants[body.code[3].arg as usize],
        Constant::Text("size".to_string()),
        "DEF_FIELD names the field; its value came off the stack"
    );
    assert_eq!(
        chunk.constants[body.code[4].arg as usize],
        Constant::Nothing,
        "a field with no `default` starts as nothing, which is what the VM gives it"
    );
    assert_eq!(
        chunk.constants[body.code[5].arg as usize],
        Constant::Text("label".to_string()),
        "and the second field is named the same way"
    );

    let bytes = chunk.encode();
    assert_eq!(
        Chunk::decode(&bytes).expect("should round trip"),
        chunk,
        "a compiled default must survive a round trip"
    );
    assert!(
        disassemble(&chunk).contains("ADD"),
        "the default should be visible in the disassembly"
    );
}

#[test]
fn try_compiles_to_a_handler_instruction_and_separate_blocks() {
    let chunk =
        compile_source("try\n    say 1\ncatch problem\n    say problem\nfinally\n    say 2\nend\n")
            .expect("program should compile");

    let try_instruction = chunk
        .main
        .code
        .iter()
        .find(|i| i.opcode == Opcode::Try)
        .expect("try must compile to a TRY instruction");

    let catch = &chunk.main.blocks[try_instruction.arg as usize];
    assert_eq!(catch.kind, BlockKind::CatchBody);
    let finally = &chunk.main.blocks[try_instruction.aux as usize];
    assert_eq!(finally.kind, BlockKind::FinallyBody);
}

#[test]
fn try_without_handlers_names_no_blocks() {
    let chunk = compile_source("try\n    say 1\nend\n").expect("program should compile");

    let try_instruction = chunk
        .main
        .code
        .iter()
        .find(|i| i.opcode == Opcode::Try)
        .expect("try must compile to a TRY instruction");

    assert_eq!(try_instruction.arg, u32::MAX, "no catch block is absent");
    assert_eq!(try_instruction.aux, u32::MAX, "no finally block is absent");
    assert!(
        chunk.main.blocks.is_empty(),
        "a try with no handlers needs no blocks"
    );
}

#[test]
fn a_constant_compiles_to_its_own_instruction_not_to_a_store() {
    let chunk = compile_source("constant TAU to 6.28318\nset TAU to 7\nsay TAU\n")
        .expect("program should compile");

    assert_eq!(
        names(&chunk.main),
        vec![
            "PUSH_CONST",
            "DECLARE_CONST",
            "PUSH_CONST",
            "STORE",
            "LOAD",
            "SAY"
        ],
        "a declaration and an assignment must not compile to the same instruction"
    );

    let declaration = chunk.main.code[1];
    assert_eq!(
        chunk.constants[declaration.arg as usize],
        Constant::Text("TAU".to_string()),
        "DECLARE_CONST names the constant it binds"
    );
    assert!(
        declaration.arg == chunk.main.code[3].arg,
        "one name is one constant-pool entry, so both instructions name it"
    );
    assert!(
        disassemble(&chunk).contains("DECLARE_CONST    "),
        "the disassembler should print the new mnemonic:\n{}",
        disassemble(&chunk)
    );
}

#[test]
fn edge_the_constant_instruction_has_a_byte_of_its_own() {
    assert_eq!(
        Opcode::DeclareConst.to_byte(),
        46,
        "the byte value is part of the file format and is never reused"
    );
    assert_eq!(
        Opcode::from_byte(Opcode::DeclareConst.to_byte()),
        Some(Opcode::DeclareConst),
        "the byte should decode back to the instruction"
    );
    assert_eq!(
        Opcode::ALL.len(),
        Opcode::DeclareConst.to_byte() as usize + 1,
        "the table is indexed by byte value, so it ends at the last instruction"
    );
    assert!(
        Opcode::DeclareConst.has_operand() && Opcode::DeclareConst.takes_constant_index(),
        "DECLARE_CONST names the constant it binds through `arg`, like STORE"
    );
}

#[test]
fn edge_a_version_2_file_is_refused_because_it_cannot_say_a_name_is_read_only() {
    let mut bytes = compile_source("constant TAU to 6.28318\n")
        .expect("program should compile")
        .encode();
    bytes[4..6].copy_from_slice(&2u16.to_le_bytes());

    let error = Chunk::decode(&bytes).expect_err("a version-2 file must not be accepted");
    let message = error.message();
    assert!(
        message.contains("version 2") && message.contains(&FORMAT_VERSION.to_string()),
        "the error should name both the file's version and this build's, got: {message}"
    );
}

#[test]
fn imports_compile_to_an_import_per_item_bound_to_its_alias() {
    let chunk = compile_source("import MathUtils, files to net\n").expect("program should compile");

    assert_eq!(
        names(&chunk.main),
        vec!["IMPORT", "STORE", "IMPORT", "STORE"],
        "each import binds a name"
    );
    assert_eq!(
        chunk.constants[chunk.main.code[0].arg as usize],
        Constant::Text("MathUtils".to_string())
    );
    assert_eq!(
        chunk.constants[chunk.main.code[3].arg as usize],
        Constant::Text("net".to_string()),
        "an import binds its alias, not its module name"
    );
}

#[test]
fn edge_the_same_name_is_one_constant_so_the_pool_is_canonical() {
    let chunk = compile_source("set x to 1\nset x to 2\nsay x\n").expect("program should compile");

    let text_constants = chunk
        .constants
        .iter()
        .filter(|c| **c == Constant::Text("x".to_string()))
        .count();
    assert_eq!(
        text_constants, 1,
        "a name used three times should be one constant: {:?}",
        chunk.constants
    );
    let named: Vec<u32> = chunk
        .main
        .code
        .iter()
        .filter(|i| matches!(i.opcode, Opcode::Load | Opcode::Store))
        .map(|i| i.arg)
        .collect();
    assert_eq!(
        named[0], named[1],
        "both stores should name the same constant, got {named:?}"
    );
    assert_eq!(
        named[0], named[2],
        "the load should name it too, got {named:?}"
    );
    assert_eq!(
        chunk.constants[named[0] as usize],
        Constant::Text("x".to_string()),
        "and that constant is the name, not a literal: {:?}",
        chunk.constants
    );
}

#[test]
fn edge_duplicate_record_keys_and_missing_names_still_compile() {
    let chunk = compile_source("set rec to {a: 1, a: 2}\nsay rec.a\nsay rec.missing\n")
        .expect("a repeated key is the compiler's business, not the format's");

    let record = chunk
        .main
        .code
        .iter()
        .find(|i| i.opcode == Opcode::BuildRecord)
        .expect("a record literal builds a record");
    assert_eq!(
        record.arg, 2,
        "BUILD_RECORD carries its pair count, both `a` pairs included"
    );
    assert_eq!(
        chunk
            .constants
            .iter()
            .filter(|c| **c == Constant::Text("a".to_string()))
            .count(),
        1,
        "the key written twice is one constant: {:?}",
        chunk.constants
    );
}

#[test]
fn edge_unicode_and_escapes_survive_the_constant_pool_byte_for_byte() {
    let source =
        "say \"emoji 🎉 cjk 日本語 rtl \u{202E}quote \\\" backslash \\\\ newline \n done\"\n";
    let chunk = compile_source(source).expect("unicode program should compile");

    let text = match &chunk.constants[0] {
        Constant::Text(text) => text.clone(),
        other => panic!("expected a text constant, got {other:?}"),
    };
    assert!(text.contains('🎉'), "emoji was lost: {text:?}");
    assert!(text.contains("日本語"), "CJK was lost: {text:?}");
    assert!(text.contains('\u{202E}'), "RTL mark was lost: {text:?}");
    // The lexer resolves `\"` and `\\` before the constant pool sees them, so
    // the pooled text holds the characters themselves.
    assert!(
        text.contains("quote \""),
        "an escaped quote was lost: {text:?}"
    );
    assert!(
        text.contains("backslash \\"),
        "an escaped backslash was lost: {text:?}"
    );
    assert!(
        text.contains('\n'),
        "an escape-produced newline was lost: {text:?}"
    );

    let bytes = chunk.encode();
    let decoded = Chunk::decode(&bytes).expect("unicode constants should decode");
    assert_eq!(decoded, chunk);
    assert_eq!(decoded.encode(), bytes, "re-encoding changed the bytes");
}

#[test]
fn edge_numeric_boundaries_survive_the_constant_pool() {
    for literal in ["0", "-0.0", "9007199254740993", "1e308", "3.14159", "0.1"] {
        let chunk =
            compile_source(&format!("set n to {literal}\n")).expect("number should compile");
        match chunk.constants[0] {
            Constant::Number(n) => {
                assert!(
                    n.is_finite(),
                    "{literal} decoded to a non-finite number: {n}"
                );
            }
            ref other => panic!("expected a number constant for {literal}, got {other:?}"),
        }
        // The documented layout, counted out. 50 bytes of overhead: magic,
        // version, constant count, one number constant, one text constant for
        // the name `n`, the main block header — kind, arity, parameter count,
        // name length and the name `main` — and its zero child count. Every
        // instruction is then exactly INSTRUCTION_SIZE bytes.
        let bytes = chunk.encode();
        assert_eq!(
            bytes.len(),
            50 + 13 * chunk.main.code.len(),
            "the file must be the documented size for {literal}"
        );
        if literal.starts_with('-') {
            assert!(
                ops(&chunk.main).contains(&Opcode::Neg),
                "a negative literal is a negation of a positive constant: {literal}"
            );
        }
        let decoded = Chunk::decode(&bytes).expect("numeric constants should decode");
        assert_eq!(decoded, chunk, "{literal} did not survive the round trip");
    }
}

#[test]
fn compile_refuses_a_program_the_frontend_rejects() {
    let error = compile_source("say \"unterminated\n").expect_err("a bad program must not compile");

    assert!(
        !error.message().is_empty(),
        "a rejected program must say why, got {error:?}"
    );
}

#[test]
fn edge_deeply_nested_declarations_still_compile_within_a_bounded_block_tree() {
    let mut source = String::new();
    for depth in 0..8 {
        source.push_str(&format!("{}to f{depth}()\n", "  ".repeat(depth)));
    }
    for depth in (0..8).rev() {
        source.push_str(&format!("{}end\n", "  ".repeat(depth)));
    }

    let chunk = compile_source(&source).expect("8 nested declarations should compile");
    let functions = blocks_of_kind(&chunk.main, BlockKind::Function);
    assert_eq!(functions.len(), 8, "one block per declaration");

    let bytes = chunk.encode();
    assert_eq!(
        Chunk::decode(&bytes).expect("nested blocks should decode"),
        chunk
    );
}

// ---------------------------------------------------------------------------
// The CLI surface
// ---------------------------------------------------------------------------

#[test]
fn rb_compile_writes_a_rbc_that_rb_dis_reads_back() {
    let dir = scratch_dir("cli-round-trip");
    let source = dir.join("hello.rb");
    fs::write(&source, "say \"Hello, World!\"\n").expect("scratch source should be writable");

    let out = dir.join("hello.rbc");
    let compile = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("compile")
        .arg(&source)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("rb should be runnable");

    assert!(
        compile.status.success(),
        "rb compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(out.exists(), "rb compile wrote no .rbc file");

    let bytes = fs::read(&out).expect("the .rbc should be readable");
    assert_eq!(&bytes[..MAGIC.len()], &MAGIC, "the file is not bytecode");

    let dis = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("dis")
        .arg(&out)
        .output()
        .expect("rb should be runnable");

    assert!(
        dis.status.success(),
        "rb dis failed: {}",
        String::from_utf8_lossy(&dis.stderr)
    );

    let first = String::from_utf8(dis.stdout).expect("disassembly is text");
    assert_eq!(
        first,
        disassemble(&Chunk::decode(&bytes).expect("the written file should decode")),
        "rb dis should print exactly disassemble()"
    );

    let again = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("dis")
        .arg(&out)
        .output()
        .expect("rb should be runnable");
    assert_eq!(
        String::from_utf8(again.stdout).expect("disassembly is text"),
        first,
        "rb dis is not deterministic"
    );
}

/// The definition of done for this stage, end to end through the binary: every
/// file under `examples/` compiles to a `.rbc`, and `rb dis` reads each one
/// back to exactly the text `disassemble` gives.
#[test]
fn rb_compile_and_dis_cover_every_example_through_the_binary() {
    let dir = scratch_dir("cli-examples");
    let examples = example_sources();
    assert!(
        examples.len() >= 6,
        "examples/ holds {} files; the corpus should not shrink silently",
        examples.len()
    );

    for (name, source) in examples {
        let input = dir.join(&name);
        fs::write(&input, &source).expect("scratch source should be writable");
        let out = dir.join(format!("{}.rbc", name.trim_end_matches(".rb")));

        let compile = Command::new(env!("CARGO_BIN_EXE_rb"))
            .arg("compile")
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .output()
            .expect("rb should be runnable");
        assert!(
            compile.status.success(),
            "rb compile examples/{name} failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        assert!(out.exists(), "rb compile wrote no .rbc for examples/{name}");

        let bytes = fs::read(&out).expect("the .rbc should be readable");
        assert_eq!(
            bytes,
            compile_source(&source)
                .unwrap_or_else(|e| panic!("examples/{name} should compile: {e}"))
                .encode(),
            "examples/{name} compiled by the binary differs from the library"
        );

        let dis = Command::new(env!("CARGO_BIN_EXE_rb"))
            .arg("dis")
            .arg(&out)
            .output()
            .expect("rb should be runnable");
        assert!(
            dis.status.success(),
            "rb dis on examples/{name} failed: {}",
            String::from_utf8_lossy(&dis.stderr)
        );
        assert_eq!(
            String::from_utf8(dis.stdout).expect("disassembly is text"),
            disassemble(&Chunk::decode(&bytes).expect("the written file should decode")),
            "rb dis on examples/{name} did not print disassemble()"
        );
    }
}

#[test]
fn edge_rb_dis_refuses_a_source_file_and_a_missing_file() {
    let dir = scratch_dir("cli-refusals");
    let source = dir.join("hello.rb");
    fs::write(&source, "say 1\n").expect("scratch source should be writable");

    let wrong_kind = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("dis")
        .arg(&source)
        .output()
        .expect("rb should be runnable");
    assert!(
        !wrong_kind.status.success(),
        "rb dis accepted a .rb source file"
    );
    let message = String::from_utf8_lossy(&wrong_kind.stderr);
    assert!(
        message.contains(".rbc"),
        "the refusal should say what it wanted, got: {message}"
    );

    let missing = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("dis")
        .arg(dir.join("nope.rbc"))
        .output()
        .expect("rb should be runnable");
    assert!(!missing.status.success(), "rb dis accepted a missing file");
    assert!(
        !String::from_utf8_lossy(&missing.stderr).is_empty(),
        "a failed rb dis must say why"
    );
}

#[test]
fn edge_rb_compile_reports_a_bad_source_and_writes_nothing() {
    let dir = scratch_dir("cli-bad-source");
    let source = dir.join("broken.rb");
    fs::write(&source, "say \"unterminated\n").expect("scratch source should be writable");
    let out = dir.join("broken.rbc");

    let compile = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("compile")
        .arg(&source)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("rb should be runnable");

    assert!(
        !compile.status.success(),
        "rb compile accepted a program that does not parse"
    );
    assert!(!out.exists(), "a failed compile left a file behind");
    assert!(
        !String::from_utf8_lossy(&compile.stderr).is_empty(),
        "a failed compile must say why"
    );
}

#[test]
fn rb_compile_without_an_output_flag_writes_beside_the_source() {
    let dir = scratch_dir("cli-default-output");
    let source = dir.join("default_name.rb");
    fs::write(&source, "say 1\n").expect("scratch source should be writable");

    let compile = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("compile")
        .arg(&source)
        .output()
        .expect("rb should be runnable");

    assert!(
        compile.status.success(),
        "rb compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let out = dir.join("default_name.rbc");
    assert!(out.exists(), "expected {}", out.display());
    assert_eq!(
        fs::read(&out).expect("the .rbc should be readable"),
        compile_source("say 1\n")
            .expect("program should compile")
            .encode(),
        "the written bytes should be the documented encoding"
    );
}

#[test]
fn rb_help_documents_the_two_new_subcommands() {
    let out = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("help")
        .output()
        .expect("rb should be runnable");
    let help = String::from_utf8_lossy(&out.stdout);

    assert!(
        help.contains("compile"),
        "`rb help` does not mention compile:\n{help}"
    );
    assert!(
        help.contains("dis"),
        "`rb help` does not mention dis:\n{help}"
    );
}
// ---------------------------------------------------------------------------
// Constructed chunks: branches the Redblue frontend cannot reach today
// ---------------------------------------------------------------------------

use redblue::parser::{Expr, Program, Stmt};
use redblue::Span;

fn say(expr: Expr) -> Stmt {
    Stmt {
        span: Span::new(1, 1),
        statement: redblue::parser::Statement::Say(expr),
    }
}

/// `Expr::InterpolatedText` exists in the AST but no Redblue source reaches it:
/// the parser leaves the braces in the text literal (see FINDINGS.md). The
/// compiler still lowers it, and this is the only way to see that it does.
#[test]
fn edge_an_interpolated_text_becomes_build_text_over_its_parts() {
    let program = Program {
        statements: vec![say(Expr::InterpolatedText(vec![
            Expr::Text("n is ".to_string()),
            Expr::Binary {
                op: redblue::parser::BinaryOp::Add,
                left: Box::new(Expr::Number(1.0)),
                right: Box::new(Expr::Number(2.0)),
            },
        ]))],
    };

    let chunk = redblue::bytecode::compile_program(&program).expect("an AST should compile");

    assert_eq!(
        names(&chunk.main),
        vec![
            "PUSH_CONST",
            "PUSH_CONST",
            "PUSH_CONST",
            "ADD",
            "BUILD_TEXT",
            "SAY"
        ],
        "an interpolated text is its parts, then BUILD_TEXT over them"
    );
    assert_eq!(
        chunk.main.code[4].arg, 2,
        "BUILD_TEXT carries its part count"
    );
    assert_eq!(
        Chunk::decode(&chunk.encode()).expect("should round trip"),
        chunk
    );
}

/// The compile depth limit, reached with a block tree deeper than the
/// compiler will emit — the parser stops a source file 64 levels earlier, so
/// only a constructed program gets here.
#[test]
fn edge_a_program_nested_past_the_block_limit_is_refused() {
    let mut statements = vec![say(Expr::Nothing)];
    for _ in 0..(redblue::bytecode::MAX_BLOCK_DEPTH + 4) {
        statements = vec![Stmt {
            span: Span::new(1, 1),
            statement: redblue::parser::Statement::Function {
                name: "deep".to_string(),
                params: Vec::new(),
                body: statements,
            },
        }];
    }

    let error = redblue::bytecode::compile_program(&Program { statements })
        .expect_err("a program this deep must be refused, not recursed into");

    assert!(
        error.message().contains("nests"),
        "the refusal should say how deep it got, got: {error}"
    );
}

#[test]
fn edge_blocks_nested_past_the_limit_are_refused_when_decoding() {
    let mut block = Block {
        name: String::new(),
        kind: BlockKind::Main,
        arity: 0,
        params: Vec::new(),
        code: Vec::new(),
        blocks: Vec::new(),
    };
    for _ in 0..(redblue::bytecode::MAX_BLOCK_DEPTH + 4) {
        block = Block {
            name: "deep".to_string(),
            kind: BlockKind::Function,
            arity: 0,
            params: Vec::new(),
            code: Vec::new(),
            blocks: vec![block],
        };
    }

    let chunk = Chunk {
        constants: Vec::new(),
        main: block,
    };
    let error = Chunk::decode(&chunk.encode())
        .expect_err("a file nested deeper than this build writes is not one to read");

    assert!(
        error.message().contains("deep"),
        "the refusal should say how deep it got, got: {error}"
    );
}

/// A chain of empty blocks `depth` deep.
///
/// The compiler refuses to nest past `MAX_BLOCK_DEPTH` and so does the decoder,
/// but `Block`'s fields are public: a caller can still build a deeper tree, and
/// everything that walks one has to cope with it.
fn deep_block_chain(depth: usize) -> Block {
    let mut block = Block {
        name: String::new(),
        kind: BlockKind::Main,
        arity: 0,
        params: Vec::new(),
        code: Vec::new(),
        blocks: Vec::new(),
    };
    for level in 0..depth {
        block = Block {
            name: format!("level{level}"),
            kind: BlockKind::Function,
            arity: 0,
            params: Vec::new(),
            code: Vec::new(),
            blocks: vec![block],
        };
    }
    block
}

/// Three thousand levels: deeper than the compiler emits, and deeper than a
/// stack holds one frame per level. The walk over the tree — `all_blocks`,
/// `encode` and `disassemble` — keeps its pending blocks on the heap, so this
/// finishes. The worker thread's stack is deliberately small: if any of the
/// three recursed per nesting level, this test would abort the process rather
/// than pass quietly.
#[test]
fn edge_a_tree_deeper_than_the_block_limit_still_renders_and_walks() {
    let depth = 3_000usize;
    let chunk = Chunk {
        constants: Vec::new(),
        main: deep_block_chain(depth),
    };

    let worker = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || {
            let blocks = chunk.all_blocks().len();
            let bytes = chunk.encode().len();
            let text = disassemble(&chunk);
            (blocks, bytes, text)
        })
        .expect("a worker thread should start");

    let (blocks, bytes, text) = worker
        .join()
        .expect("no walk over the tree may recurse per nesting level");

    assert_eq!(
        blocks,
        depth + 1,
        "every block should be reached exactly once, `main` included"
    );
    assert!(bytes > 0, "a tree past the limit still encodes");
    assert!(
        text.contains("main.blocks[0].blocks[0]"),
        "the disassembly should name nested blocks:\n{text:.200}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| line.contains("(function, arity 0)"))
            .count(),
        depth,
        "every level should be rendered exactly once"
    );
}

/// Every index `rb dis` prints is checked, and the operands that name blocks
/// used to be the ones it did not check. A corrupt operand has to be reported
/// where a reader looks rather than read as if it were sound.
#[test]
fn edge_the_disassembler_reports_a_block_index_out_of_range() {
    let chunk = Chunk {
        constants: vec![
            Constant::Text("Parent".to_string()),
            Constant::Text("g".to_string()),
        ],
        main: Block {
            name: "main".to_string(),
            kind: BlockKind::Main,
            arity: 0,
            params: Vec::new(),
            code: vec![
                // In range: block 0 is a block of this one.
                Instruction {
                    opcode: Opcode::DefFunction,
                    arg: 0,
                    aux: 0,
                    line: 1,
                },
                // Past the end of a list that holds one block.
                Instruction {
                    opcode: Opcode::DefMethod,
                    arg: 3,
                    aux: 0,
                    line: 2,
                },
                Instruction {
                    opcode: Opcode::Test,
                    arg: 7,
                    aux: 0,
                    line: 3,
                },
                // The parent names a real constant; the block is not there.
                Instruction {
                    opcode: Opcode::DefObject,
                    arg: 9,
                    aux: 0,
                    line: 4,
                },
                // A parent index past the end of the pool.
                Instruction {
                    opcode: Opcode::DefObject,
                    arg: 0,
                    aux: 5,
                    line: 5,
                },
                // A catch block that is not there, and a finally that is
                // legitimately absent.
                Instruction {
                    opcode: Opcode::Try,
                    arg: 4,
                    aux: NO_BLOCK,
                    line: 6,
                },
            ],
            blocks: vec![Block {
                name: "f".to_string(),
                kind: BlockKind::Function,
                arity: 0,
                params: Vec::new(),
                code: Vec::new(),
                blocks: Vec::new(),
            }],
        },
    };

    let text = disassemble(&chunk);

    assert!(
        text.contains("block 0 (function, `f`)"),
        "a block index that is in range should be named:\n{text}"
    );
    assert!(
        text.contains("extends Parent"),
        "an object with a parent in range should say so:\n{text}"
    );
    for past in ["block 3", "block 7", "block 9", "catch block 4"] {
        assert!(
            text.contains(&format!("{past} is out of range")),
            "{past} is past this block's one block and should be reported:\n{text}"
        );
    }
    assert!(
        text.contains("constant 5 is out of range"),
        "a parent index past the pool should be reported:\n{text}"
    );
    assert!(
        !text.contains("finally block"),
        "NO_BLOCK is a legal operand, not an out-of-range one:\n{text}"
    );

    // The operands are not the decoder's business: the file is structurally
    // sound, so it decodes, and the report is still there afterwards.
    let decoded = Chunk::decode(&chunk.encode()).expect("this file is well formed");
    assert_eq!(
        disassemble(&decoded),
        text,
        "the report must survive a round trip"
    );
}

#[test]
fn edge_an_unassigned_byte_in_any_enum_is_rejected() {
    // The three tagged bytes a decoder can meet: the block kind, a constant
    // tag and a yes/no value. The offsets are counted from the header: 4 for
    // the magic, 2 for the version, 4 for the constant count, then the pool.
    let empty_main = || Block {
        name: "main".to_string(),
        kind: BlockKind::Main,
        arity: 0,
        params: Vec::new(),
        code: Vec::new(),
        blocks: Vec::new(),
    };

    let nothing = Chunk {
        constants: vec![Constant::Nothing],
        main: empty_main(),
    };
    assert_eq!(
        nothing.encode()[10],
        0,
        "one nothing constant is one tag byte"
    );

    let mut bad_kind = nothing.encode();
    bad_kind[11] = 200;
    assert!(
        Chunk::decode(&bad_kind)
            .expect_err("an unknown block kind must be refused")
            .message()
            .contains("block kind"),
        "a file whose block kind is 200 decoded"
    );

    let mut bad_tag = nothing.encode();
    bad_tag[10] = 99;
    assert!(
        Chunk::decode(&bad_tag)
            .expect_err("an unknown constant tag must be refused")
            .message()
            .contains("constant tag"),
        "a file whose constant tag is 99 decoded"
    );

    let yesno = Chunk {
        constants: vec![Constant::YesNo(true)],
        main: empty_main(),
    };
    let mut bad_yesno = yesno.encode();
    bad_yesno[11] = 7;
    assert!(
        Chunk::decode(&bad_yesno)
            .expect_err("an unknown yes/no byte must be refused")
            .message()
            .contains("yes/no"),
        "a file whose yes/no byte is 7 decoded"
    );
}

#[test]
fn edge_trailing_bytes_after_the_last_block_are_refused() {
    let mut bytes = compile_source("say 1\n")
        .expect("program should compile")
        .encode();
    bytes.push(0);

    let error = Chunk::decode(&bytes).expect_err("one byte past the end is not a valid file");

    assert!(
        error.message().contains("follow"),
        "the error should say there is more in the file than it read, got: {error}"
    );
}

#[test]
fn edge_the_disassembler_reports_an_out_of_range_operand_instead_of_panicking() {
    let chunk = Chunk {
        constants: vec![Constant::Text("only".to_string())],
        main: Block {
            name: "main".to_string(),
            kind: BlockKind::Main,
            arity: 0,
            params: Vec::new(),
            code: vec![
                Instruction {
                    opcode: Opcode::PushConst,
                    arg: 9,
                    aux: 0,
                    line: 1,
                },
                Instruction {
                    opcode: Opcode::Jump,
                    arg: 40,
                    aux: 0,
                    line: 1,
                },
            ],
            blocks: Vec::new(),
        },
    };

    let text = disassemble(&chunk);

    assert!(
        text.contains("constant 9 is out of range"),
        "an operand past the pool should be reported:\n{text}"
    );
    assert!(
        text.contains("target 40 is outside"),
        "a jump past the block should be reported:\n{text}"
    );
}
