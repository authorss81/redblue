//! Windows main-thread stack: 1 MiB there against 8 MiB everywhere else.
//!
//! A deeply nested program (e.g. 65 nested function literals, past the
//! parser's own block budget) overflows the small Windows main stack in debug
//! builds while Linux diagnoses it cleanly — so the same test is green on one
//! platform and an abort on the other, for no reason in the language itself.
//! Reserving the Linux-sized stack on Windows aligns the two: virtual address
//! space only, committed on demand, zero cost unless deeply recursed into.
//! Non-Windows targets are untouched.
fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo::rustc-link-arg=/STACK:8388608");
    }
}
