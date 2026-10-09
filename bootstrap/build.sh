#!/usr/bin/env bash
# The S4 release build: the compiler the release carries is written by
# bootstrap/compiler.rb itself.
#
# From a clean checkout, with a Rust toolchain installed:
#
#     ./bootstrap/build.sh                 # build the release compiler
#     ./bootstrap/build.sh --rollback      # cut it from the Rust frontend
#
# What it does, and why each step refuses rather than continuing:
#
#   1. `cargo build --release` builds the `rb` that runs the ladder. This is
#      stage 0 and it is Rust; nothing in this script pretends otherwise. The
#      ladder it starts is the ladder itself, not a shortcut past it.
#   2. `rb bootstrap <out-dir>` writes stage1.rbc, stage2.rbc and compiler.rbc.
#      Stage 1 is the Rust frontend over bootstrap/compiler.rb; stage 2 is that
#      file compiled by itself, running as bytecode; compiler.rbc is stage 2's
#      bytes. A stage 2 that is not byte-identical to stage 1 is a non-zero exit
#      and nothing shipped — that is the whole of the ladder's rule.
#   3. The fixed point is re-checked here with `cmp`, from the files on disk
#      rather than from the build's return value, so a `rb bootstrap` that
#      reported success having written the wrong file is still caught.
#   4. The shipped compiler is run once, over examples/hello.rb, so a `.rbc`
#      that is byte-correct and compiles nothing cannot ship.
#
# `--rollback` changes what `rb bootstrap` does *after* the ladder: steps 1 and
# 2 still run in full, including the self-hosted stage 2, and the flag then
# overwrites the shipped file with the Rust frontend's bytes. It is deliberately
# not a shortcut past the ladder — a rollback taken on a compiler that cannot
# fix-point would fail at step 2, not fall back to the frontend.
#
# So what `--rollback` is for is a machine where the frontend's bytes have
# changed since the compiler was last verified — or a frontend that has been
# tampered with, or lost. The ladder still has to hold for the release to be
# cut at all, but the shipped file can then be re-derived from the frontend and
# compared against the stage 2 the ladder just verified
# (`bootstrap::rollback_onto`, which refuses to write bytes that differ).
#
# It is a rollback rather than a second compiler because the bytes are the
# same, which steps 3 and 4 check — a rollback that produced different bytes
# would be a way to ship something the ladder never approved, and the CLI
# refuses it before the write.
#
# Cost: one self-compilation is ~16 s in a release build and ~3 min in a debug
# build. REDBLUE_MAX_STEPS overrides the step budget if a future compiler has
# outgrown the default in `src/interpreter.rs`.

set -euo pipefail

rollback=0
for arg in "$@"; do
    case "$arg" in
        --rollback | -r) rollback=1 ;;
        *)
            echo "build.sh does not take $arg" >&2
            exit 1
            ;;
    esac
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
out_dir="${REDBLUE_BOOTSTRAP_OUT:-target/release-bootstrap}"

echo "==> stage 0: cargo build --release"
cargo build --release --locked

# The release job runs this on Windows too, where cargo names the binary
# `rb.exe`. Named rather than assumed: a path that does not exist here would
# fail three steps later, on the ladder, with the ladder named as the culprit.
if [ -x "./target/release/rb" ]; then
    rb="./target/release/rb"
elif [ -x "./target/release/rb.exe" ]; then
    rb="./target/release/rb.exe"
else
    echo "no rb binary in target/release after cargo build --release" >&2
    exit 1
fi

echo "==> S4: stage 1, stage 2, the fixed point"
if [ "$rollback" -eq 1 ]; then
    "$rb" bootstrap "$out_dir" --rollback
else
    "$rb" bootstrap "$out_dir"
fi

echo "==> re-checking the fixed point from the files on disk"
cmp "$out_dir/stage1.rbc" "$out_dir/stage2.rbc"
cmp "$out_dir/stage2.rbc" "$out_dir/compiler.rbc"
echo "    stage1.rbc == stage2.rbc == compiler.rbc"

echo "==> running the shipped compiler once, so it is known to compile"
smoke_dir="$out_dir/smoke"
mkdir -p "$smoke_dir"
"$rb" vm "$out_dir/compiler.rbc" examples/hello.rb "$smoke_dir/hello.rbc"
test -s "$smoke_dir/hello.rbc"

echo
echo "The compiler the release carries: $out_dir/compiler.rbc"
echo "Use it with:  rb vm $out_dir/compiler.rbc <program.rb> <program.rbc>"
