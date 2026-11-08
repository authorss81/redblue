//! `rb-bench` — the measuring harness, outside the `cargo test` gate path.
//!
//! The workloads and their correctness checks live in `mod.rs`, which the test
//! suite also reads. This binary only parses arguments, prints the table, and
//! writes `BENCH.md` when all four workloads ran.

#[path = "mod.rs"]
mod bench;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(not(test))]
    {
        if arguments.first().map(String::as_str) == Some("--worker") {
            std::process::exit(bench::run_worker(&arguments[1..]));
        }
    }
    std::process::exit(bench::run_cli(&arguments));
}
