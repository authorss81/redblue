//! The WebAssembly playground boundary.
//!
//! Redblue runs two ways from the same interpreter: as the native `rb` binary,
//! and — since this module existed — compiled to `wasm32-unknown-unknown` and
//! loaded by the page in `wasm/index.html`. Nothing here changes how a program
//! is parsed, analyzed or executed. It is the *output* boundary and the *host
//! call* boundary, and nothing else:
//!
//! - [`emit`] is the single point where a program's bytes leave the
//!   interpreter. Natively it writes to stdout, exactly as `println!` and
//!   `print!` did; with a capture open it appends to a buffer the host reads.
//!   Same bytes, same order, same line-buffering boundaries — the difference is
//!   which sink receives them.
//! - [`run_program`] is the whole pipeline with that capture open, so a caller
//!   gets a program's output and a rendered failure as values.
//! - The `rb_*` functions at the bottom are the exported C ABI. They are plain
//!   `extern "C"` over `wasm32-unknown-unknown`'s linear memory: no bindgen, no
//!   `wasm-bindgen`, no JavaScript toolchain. `wasm/redblue.js` is the glue that
//!   drives them, and `wasm/check-examples.js` is what proves the two pipelines
//!   agree.
//!
//! Everything compiles and is testable on the host. `tests/wasm_test.rs` walks
//! the ABI natively so a mistake in the pointer arithmetic fails in
//! `cargo test`, not in a browser.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Whether program output is being collected instead of written to stdout.
static CAPTURING: AtomicBool = AtomicBool::new(false);

/// Set when a run tried to [`emit`] past [`MAX_OUTPUT_BYTES`]. A run that
/// overflows is reported as a failure rather than silently losing the tail,
/// because a playground showing the first 4 MiB of an endless loop and calling
/// it a success would be lying about what the program did.
static OVERFLOW: AtomicBool = AtomicBool::new(false);

/// The buffer a capture collects into. A [`Mutex`] rather than a `RefCell`
/// because `say` runs on the interpreter thread, not the thread that opened the
/// capture, and on wasm there is only one thread anyway.
static CAPTURED: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// The most output one run may produce, in bytes.
///
/// The playground hands its output to a host as an *address*, not a copy, so the
/// bytes behind that address have to stay put — see [`HostBuffer`]. That buffer
/// is allocated once and reused, which is what makes the address stable, and a
/// reused buffer has to have a size. This is it. A program past it is refused
/// with a message, on the same `Error::Limit` channel [`crate::interpreter::MAX_CALL_DEPTH`]
/// uses, so a Redblue program can `catch` it like any other limit.
pub const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// The most rendered error text one run may leave for a host. An error carries a
/// source line and a caret under it, so this is generous for anything a person
/// wrote and tight enough that one enormous generated line cannot use it up.
///
/// Public because a host has to size its own buffer with it — that is what makes
/// [`rb_run_into`]'s length cells worth anything.
pub const MAX_ERROR_BYTES: usize = 64 * 1024;

/// Appended to text that did not fit its buffer, so a host sees that it is
/// looking at part of a message rather than all of it. The first bytes are the
/// ones that name the failure, so a truncated error is still useful.
const TRUNCATION_MARKER: &str = "\n[truncated: the message does not fit the buffer]";

fn output_buffer() -> MutexGuard<'static, Vec<u8>> {
    CAPTURED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Writes a program's output.
///
/// This is what `say` and `print` call, as a statement and as the builtins of
/// the same names. Natively it is a plain write to stdout with no trailing
/// newline of its own — `say` supplies the newline in the text it hands over —
/// so the byte stream is exactly what `println!`/`print!` were producing,
/// including when stdout flushes, because a [`Write::write_all`] of `"line\n"`
/// reaches a [`LineWriter`](std::io::LineWriter) in the same shape `println!`
/// did.
pub fn emit(text: &str) {
    if CAPTURING.load(Ordering::Relaxed) {
        let mut buffer = output_buffer();
        if buffer.len().saturating_add(text.len()) > MAX_OUTPUT_BYTES {
            OVERFLOW.store(true, Ordering::SeqCst);
            return;
        }
        buffer.extend_from_slice(text.as_bytes());
        return;
    }
    // `print!` ignores write failures too, and a Redblue program must not be
    // able to abort the host process over a closed pipe.
    let _ = std::io::stdout().write_all(text.as_bytes());
}

/// Serializes whole runs. There is one capture buffer and one program's output
/// goes in it, so two runs cannot overlap — on wasm there is one host thread
/// and the point is moot, but the module is also compiled natively where
/// `cargo test` runs its tests in parallel, and without this two of them would
/// divide one program's output between them. The lock is held across the run,
/// not around [`begin_capture`] and [`take_capture`] separately.
static RUN_LOCK: Mutex<()> = Mutex::new(());

/// The one lock every run, allocation and read of the ABI takes.
///
/// Not only `rb_run` and `run_program`. The *read* half has to be serialised
/// too, and it always was only half-serialised: `rb_run` held this while it
/// wrote both channels, and the accessors that hand out their addresses and
/// lengths held nothing, so two native tests could have one thread's `rb_run`
/// rewrite the bytes another thread was mid-way through reading. Every entry
/// point into the ABI now takes it, and the ones that copy ([`rb_read_output`],
/// [`rb_read_error`]) take it across the whole copy rather than across the
/// call that starts it.
fn run_lock() -> MutexGuard<'static, ()> {
    RUN_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts a capture. The buffer is emptied first, so a capture never inherits
/// the previous one's bytes. Callers go through [`run_program`] or [`rb_run`],
/// which hold [`RUN_LOCK`]; reaching for these directly is only for a caller
/// that has serialised its own runs.
pub fn begin_capture() {
    output_buffer().clear();
    OVERFLOW.store(false, Ordering::SeqCst);
    CAPTURING.store(true, Ordering::SeqCst);
}

/// Stops a capture and takes everything collected since [`begin_capture`].
fn take_capture() -> String {
    CAPTURING.store(false, Ordering::SeqCst);
    let bytes = std::mem::take(&mut *output_buffer());
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Puts the capture back the way it was found however this run ended — a clean
/// return, a failure, or a panic unwinding out of the interpreter.
///
/// Without it a panic leaves `CAPTURING` set, and every later run in the same
/// process would append to a capture nobody is reading, which is the one way a
/// panic can cost a host more than the run that caused it.
struct CaptureGuard;

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURING.store(false, Ordering::SeqCst);
        OVERFLOW.store(false, Ordering::SeqCst);
    }
}

/// What one run produced: every byte it wrote, and the rendered failure if it
/// failed. They are independent. A program that printed three lines and then
/// divided by zero has three lines *and* a failure, and a host showing only the
/// message would look like a program that printed nothing at all.
struct RunOutcome {
    printed: String,
    failure: Option<String>,
}

/// Runs `source` through the whole pipeline with program output captured.
///
/// There is no third outcome: a Redblue program cannot panic its way out of
/// here. An unwind out of the interpreter is caught and reported like any other
/// failure, because the panic hook's stderr line is not something a page can
/// show and a module that trapped is a module that cannot run again.
fn run_outcome(source: &str) -> RunOutcome {
    begin_capture();
    let guard = CaptureGuard;

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::run_source_value(source)
    }));

    let printed = take_capture();
    let overflowed = OVERFLOW.load(Ordering::SeqCst);
    drop(guard);

    if overflowed {
        // The output collected so far is discarded rather than shown: a run that
        // passed the limit has no defined output, and showing its prefix would
        // claim the program had finished printing.
        //
        // The same limit, and deliberately the same sentence, as the one
        // `Vm::charge_output` raises for a program whose *buffered* `say` lines
        // passed it — see [`crate::interpreter::Vm::charge_output`]. Both builds
        // have to stop at the same number, and a program that flooded through
        // `print` rather than `say` should not read as a different failure for
        // having spelled it the other way.
        return RunOutcome {
            printed: String::new(),
            failure: Some(format!(
                "Output past the limit of {MAX_OUTPUT_BYTES} bytes: this program printed more \
                 than the interpreter will hold. Nothing is shown, because a program that wrote \
                 past the limit has no output to show."
            )),
        };
    }

    match outcome {
        Ok(Ok(_)) => RunOutcome {
            printed,
            failure: None,
        },
        Ok(Err(error)) => RunOutcome {
            printed,
            failure: Some(error.render(source, None)),
        },
        Err(panic) => RunOutcome {
            printed,
            failure: Some(format!(
                "The interpreter stopped unexpectedly: {}. This is a fault in Redblue itself, not \
                 something the program did.",
                panic_message(&panic)
            )),
        },
    }
}

/// The text of a caught panic, or a stand-in when the payload is not one.
///
/// `panic_any` can carry anything, so this cannot assume a `String`: a payload
/// it cannot read still has to produce a sentence the page can show.
fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = panic.downcast_ref::<&str>() {
        return (*text).to_string();
    }
    if let Some(text) = panic.downcast_ref::<String>() {
        return text.clone();
    }
    "no message".to_string()
}

/// Runs `source` through the whole pipeline — lexer, parser, analyzer, VM —
/// with program output captured, and returns the bytes.
///
/// On success the string is byte-for-byte what the native `rb` binary writes
/// for the same program. On failure it is the interpreter's rendered error, the
/// same `kind: message` plus source line and caret that `rb run` prints. The
/// output the program managed to write before failing is not in the string:
/// [`rb_run`] keeps that separately, because a caller asking "did this run
/// work" is not the same caller as a page showing the user what happened.
pub fn run_program(source: &str) -> Result<String, String> {
    let _run = run_lock();
    let outcome = run_outcome(source);
    match outcome.failure {
        Some(message) => Err(message),
        None => Ok(outcome.printed),
    }
}

/// The keyword list, newline-separated, as `rb keywords` prints it. Built once
/// and kept, because the exported accessor hands out a pointer into it.
static KEYWORD_LIST: OnceLock<String> = OnceLock::new();

fn keyword_list() -> &'static String {
    KEYWORD_LIST.get_or_init(|| {
        let mut out = String::new();
        for keyword in crate::lexer::Lexer::keywords() {
            out.push_str(keyword);
            out.push('\n');
        }
        out
    })
}

/// The crate version the playground page shows.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The keyword list, newline-separated, as `rb keywords` prints it.
pub fn keywords() -> String {
    keyword_list().clone()
}

// ---------------------------------------------------------------------------
// The exported C ABI
//
// Status codes, so a host never has to read the error buffer to find out
// whether it succeeded:
//
//   0   the program ran; the output channel holds what it wrote
//   1   the program failed; the error channel holds the rendered error
//  -1   the call itself was refused (null pointer, negative length, an
//       allocation past `MAX_SOURCE_BYTES`)
//
// Every accessor takes [`RUN_LOCK`](run_lock), so a read never overlaps the
// write of a run happening on another thread. Two ways to read a channel:
//
//   - `rb_read_output` / `rb_read_error` **copy** the bytes out under that lock
//     and return them. This is the one a host should use, and the one the
//     JavaScript in `wasm/redblue.js` uses, because the copy cannot be raced by
//     anything.
//   - `rb_output_ptr` / `rb_output_len` / `rb_error_ptr` / `rb_error_len` hand
//     out an address. Each channel owns one allocation, made at the first run
//     and reused by every run after, so an address stays valid — but only until
//     the next `rb_run`, and only for a host that is not running two at once.
//     Each call is serialised; the *pair* of calls, and the bytes between them,
//     cannot be.
// ---------------------------------------------------------------------------

/// One channel of host-visible bytes.
///
/// This exists because a pointer cannot be borrowed from a dropped guard. A
/// `Vec` behind a `Mutex` looks like it would do: `guard.as_ptr()` is an
/// address into an allocation the `static` still owns, so the bytes outlive the
/// guard — but nothing promises the allocation is still *there* when the host
/// reads, because the lock is released the moment the accessor returns and the
/// next writer can move or grow it. Handing out a pointer and then dropping the
/// only thing that could keep the memory put is exactly the mistake an FFI
/// boundary exists to prevent.
///
/// So the allocation is made once, at the first run, and never resized: the
/// bytes live in a `Box` behind a `&'static HostBuffer`, and every pointer this
/// channel hands out is derived from that one allocation rather than from any
/// temporary. `set` is the only writer, every caller of it holds the
/// [`run_lock`], and so does every reader — which is what lets this type be
/// `Sync` without an `unsafe impl`.
struct HostBuffer {
    /// The bytes themselves. A `Box`, so the allocation never moves and never
    /// grows — that is what makes [`Self::as_ptr`] a promise rather than a
    /// snapshot. It is reached through `&self` because a host holds `&'static
    /// HostBuffer`; the writes are guarded by the [`run_lock`], which is what
    /// lets this type be `Sync` without an `unsafe impl`.
    bytes: Box<[u8]>,
    used: AtomicUsize,
}

impl HostBuffer {
    /// A channel of `capacity` bytes, allocated on first use.
    fn new(capacity: usize) -> Self {
        // Every capacity below is larger than the truncation marker, so a
        // truncated message still ends with the marker rather than half of one.
        assert!(
            capacity > TRUNCATION_MARKER.len(),
            "a host buffer must have room for its own truncation marker"
        );
        HostBuffer {
            bytes: vec![0u8; capacity].into_boxed_slice(),
            used: AtomicUsize::new(0),
        }
    }

    /// Copies `text` in, replacing whatever was there, and reports whether every
    /// byte fitted.
    ///
    /// Truncation is only ever visible, never silent: what did not fit is
    /// replaced by [`TRUNCATION_MARKER`], so a host showing the text is showing
    /// a message that says it is a fragment. Program *output* cannot get here
    /// truncated — [`emit`] refuses to collect past [`MAX_OUTPUT_BYTES`] and
    /// [`run_outcome`] turns that into a failure — so this matters for a
    /// rendered error, whose source line can be any length a file can hold.
    fn set(&self, text: &str) -> bool {
        let source = text.as_bytes();
        let complete = source.len() <= self.bytes.len();
        let written = if complete {
            source.len()
        } else {
            self.bytes.len() - TRUNCATION_MARKER.len()
        };

        // SAFETY: every caller of `set` holds the `run_lock`, so no other thread
        // is writing or reading these bytes at this moment. `written + marker`
        // is at most `bytes.len()`, because `written` is only the short side
        // when `bytes.len() - TRUNCATION_MARKER.len()`, and the `assert!` in
        // `new` is what makes that subtraction well defined.
        unsafe {
            let target = self.bytes.as_ptr().cast_mut();
            std::ptr::copy_nonoverlapping(source.as_ptr(), target, written);
            if !complete {
                std::ptr::copy_nonoverlapping(
                    TRUNCATION_MARKER.as_ptr(),
                    target.add(written),
                    TRUNCATION_MARKER.len(),
                );
            }
        }
        self.used.store(
            written + usize::from(!complete) * TRUNCATION_MARKER.len(),
            Ordering::SeqCst,
        );

        complete
    }

    /// Where this channel's bytes begin.
    ///
    /// Always the same address for the life of the process, and never null: the
    /// buffer is allocated at its full size on first use, so a host that reads
    /// the pointer before the first run gets a usable address rather than a
    /// dangling one.
    fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr()
    }

    /// How many bytes are in it, as the `i32` the ABI speaks.
    fn len_i32(&self) -> i32 {
        // A host reads this many bytes from `as_ptr`, so a value that wrapped
        // negative on the way out of a `usize` would send it off the front of
        // the buffer. The buffers are far below `i32::MAX` and this clamp is
        // what says so rather than trusting it.
        i32::try_from(self.used.load(Ordering::SeqCst)).unwrap_or(i32::MAX)
    }

    /// Copies the live bytes into `dest` and reports how many were copied, as
    /// the `i32` the ABI speaks.
    ///
    /// A `cap` below zero asks only how long the channel is and writes nothing,
    /// which is how a host sizes a read without having to guess at
    /// [`MAX_OUTPUT_BYTES`] — the same way `fstat` answers a size query. Any
    /// other `cap` copies `min(cap, len)` bytes.
    ///
    /// # Safety
    ///
    /// `dest` must point at `cap` writable bytes when `cap` is not negative.
    /// Every caller is a host, and holding the [`run_lock`] across this call is
    /// what makes the copy something other than a read of memory another
    /// thread's `rb_run` is writing.
    unsafe fn copy_out(&self, dest: *mut u8, cap: i32) -> i32 {
        let len = self.len_i32();
        if cap < 0 {
            return len;
        }
        let count = cap.min(len);
        if count > 0 {
            // SAFETY: `dest` has `cap` writable bytes by the contract above, and
            // `count <= cap`; `self.bytes` holds at least `len >= count` live
            // bytes, and no thread is touching them under the lock.
            std::ptr::copy_nonoverlapping(self.bytes.as_ptr(), dest, count as usize);
        }
        count
    }
}

static HOST_OUTPUT: OnceLock<HostBuffer> = OnceLock::new();
static HOST_ERROR: OnceLock<HostBuffer> = OnceLock::new();

fn host_output() -> &'static HostBuffer {
    HOST_OUTPUT.get_or_init(|| HostBuffer::new(MAX_OUTPUT_BYTES))
}

fn host_error() -> &'static HostBuffer {
    HOST_ERROR.get_or_init(|| HostBuffer::new(MAX_ERROR_BYTES))
}

/// The most bytes one `rb_alloc` may reserve: a source program, or the scratch
/// buffer a host copies a channel out into.
///
/// The length crossing the ABI is an `i32`, so a host could ask for close to
/// two gigabytes in a single call — and get them, because there was nothing to
/// refuse it. One call would then have exhausted the module's memory, or the
/// host process's, before the host wrote a byte.
///
/// The size is [`MAX_OUTPUT_BYTES`] for a reason rather than by accident: a host
/// reading a *full* output channel into a buffer it asked the module for needs
/// room for [`MAX_OUTPUT_BYTES`] bytes, and a bound smaller than the channel
/// would make the biggest legal run unreadable through the sanctioned path. It
/// is far past any program a person writes into a page's editor, and what is
/// left is a number a host can print. Exceeding it is a null from [`rb_alloc`]
/// with the reason in the error channel, not an abort.
pub const MAX_SOURCE_BYTES: usize = MAX_OUTPUT_BYTES;

/// The buffers a host writes source into between [`rb_alloc`] and [`rb_run`].
///
/// An arena rather than one reused `Vec`, and the difference is the whole
/// point: one buffer cannot keep a pointer valid, because the next `rb_alloc`
/// that needs more room moves the allocation and the host's first pointer then
/// aims at freed memory. Appending instead leaves every live allocation exactly
/// where it was, so a pointer stays valid until the host releases that
/// allocation itself.
#[derive(Default)]
struct HostInput {
    /// Every allocation ever made, live or released. Nothing is ever removed,
    /// so an index into it always names the same memory.
    arena: Vec<Box<[u8]>>,
    /// Indices of released allocations, reusable by a later [`rb_alloc`].
    free: Vec<usize>,
}

static HOST_INPUT: Mutex<Option<HostInput>> = Mutex::new(None);

fn host_input() -> MutexGuard<'static, Option<HostInput>> {
    HOST_INPUT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl HostInput {
    /// Reserves `len` zeroed bytes for a host to write into and returns where,
    /// or `None` for a length past [`MAX_SOURCE_BYTES`].
    ///
    /// The bound is checked *before* anything is allocated, which is the whole
    /// of it: a check after `vec![0u8; len]` has already spent the memory the
    /// check exists to refuse.
    fn reserve(&mut self, len: usize) -> Option<*mut u8> {
        if len > MAX_SOURCE_BYTES {
            return None;
        }
        if len == 0 {
            // Nothing to reserve, so nowhere to put it and nothing to release.
            // An arena entry for it would be actively wrong: a `Box<[u8]>` of
            // no bytes has a dangling address, and two of them have the *same*
            // dangling address, so one pointer would name two allocations and
            // `rb_release` could not say which one it was given.
            return Some(std::ptr::NonNull::<u8>::dangling().as_ptr());
        }
        // A released buffer big enough is reused rather than replaced: there is
        // nowhere to give the memory back to, and a host that runs a thousand
        // programs must not grow the arena a thousand times.
        let index = match self
            .free
            .iter()
            .position(|&index| self.arena[index].len() >= len)
        {
            Some(at) => self.free.remove(at),
            None => {
                self.arena.push(vec![0u8; len].into_boxed_slice());
                self.arena.len() - 1
            }
        };

        let buffer = &mut self.arena[index];
        // Zeroed rather than whatever the last program left in it, so a host
        // that writes fewer bytes than it reserved hands over a source it
        // padded rather than one carrying the tail of the previous program.
        buffer[..len].fill(0);
        Some(buffer.as_mut_ptr())
    }

    /// Gives the allocation at `ptr` back for reuse. Reports whether there was
    /// one to release.
    ///
    /// By pointer, not by "the most recent one". A most-recent-only release
    /// cannot free a host's first buffer once it has made a second, and
    /// releasing the same one twice has to be told apart from releasing two —
    /// which a `last` index cannot do once it is overwritten. Matching on the
    /// address the host was handed makes both the second release and the
    /// out-of-order release ordinary.
    fn release(&mut self, ptr: *const u8) -> bool {
        let Some(index) = self
            .arena
            .iter()
            .position(|buffer| std::ptr::eq(buffer.as_ptr(), ptr))
        else {
            return false;
        };
        if self.free.contains(&index) {
            return false;
        }
        self.free.push(index);
        true
    }
}

/// Reserves `len` bytes for the host to write a source program into, and
/// returns where to write them. Returns null for a negative length and for a
/// length past [`MAX_SOURCE_BYTES`], and a refusal leaves the reason in the
/// error channel where the host already knows how to read it.
///
/// The pointer stays valid until the host releases *it*, by passing the same
/// pointer back to [`rb_release`], however many programs it runs and however
/// many other allocations it makes in between: the arena moves nothing, and any
/// allocation is freed on its own terms rather than the most recent one's.
#[no_mangle]
pub extern "C" fn rb_alloc(len: i32) -> *mut u8 {
    if len < 0 {
        return std::ptr::null_mut();
    }
    let _run = run_lock();
    let allocated = host_input()
        .get_or_insert_with(HostInput::default)
        .reserve(len as usize);
    match allocated {
        Some(pointer) => pointer,
        None => {
            host_error().set(&format!(
                "The playground will not hold a source of {len} bytes: its limit is \
                 {MAX_SOURCE_BYTES}. Split the program, or write it to a file and run it with \
                 the native `rb`."
            ));
            std::ptr::null_mut()
        }
    }
}

/// Releases the buffer at `ptr`, which must be one [`rb_alloc`] returned and not
/// already released. Returns `0`, or `-1` for a null pointer and for a pointer
/// that names no live allocation — including one released already, so a double
/// release is told apart from two releases.
///
/// A zero-length reservation holds nothing, is never given an arena slot, and
/// so has nothing to release: `rb_alloc(0)` gives a non-null pointer that
/// `rb_run` reads no bytes from, and `rb_release` on it answers `-1`.
///
/// The length of the allocation is not needed and is not taken: a host has the
/// address, and an address is what has to be matched.
#[no_mangle]
pub extern "C" fn rb_release(ptr: *const u8) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let _run = run_lock();
    match host_input()
        .as_mut()
        .map(|input| input.release(ptr))
        .unwrap_or(false)
    {
        true => 0,
        false => -1,
    }
}

/// Runs the `len` bytes at `ptr` and returns a status code. See the status
/// table above.
///
/// # Safety
///
/// `ptr` must point at `len` initialised bytes that stay valid for the length
/// of the call — which, from a host, means bytes written through a pointer
/// [`rb_alloc`] returned, since those stay valid until [`rb_release`] them and
/// [`rb_alloc`] never moves one. This is `unsafe` because Rust cannot check
/// it, which is what every FFI entry point is; it is not a hazard to the
/// interpreter, which copies the slice out before running anything.
#[no_mangle]
pub unsafe extern "C" fn rb_run(ptr: *const u8, len: i32) -> i32 {
    if len < 0 {
        return -1;
    }
    if len > 0 && ptr.is_null() {
        return -1;
    }

    let _run = run_lock();
    unsafe { write_channels(ptr, len) }
}

/// Where the last run's output begins. The address is stable for the life of
/// the module; only the bytes behind it change, and the next `rb_run` changes
/// them.
///
/// Serialised against every other entry point by the [`run_lock`], so a read
/// never *overlaps* a write — but the address a host holds across its own
/// `rb_output_len` call, and the bytes it reads through them, can still be
/// another run's by the time it gets there. [`rb_read_output`] copies instead
/// and is what a host should reach for; this is the zero-copy path, for a host
/// that is not running two runs at once.
#[no_mangle]
pub extern "C" fn rb_output_ptr() -> *const u8 {
    let _run = run_lock();
    host_output().as_ptr()
}

/// How many bytes of output the last run produced.
#[no_mangle]
pub extern "C" fn rb_output_len() -> i32 {
    let _run = run_lock();
    host_output().len_i32()
}

/// Where the last run's error message begins. The address is stable for the life
/// of the module; only the bytes behind it change. Read it through
/// [`rb_read_error`] rather than here if the host can run two programs at once;
/// see [`rb_output_ptr`].
#[no_mangle]
pub extern "C" fn rb_error_ptr() -> *const u8 {
    let _run = run_lock();
    host_error().as_ptr()
}

/// How many bytes of error message the last run produced.
#[no_mangle]
pub extern "C" fn rb_error_len() -> i32 {
    let _run = run_lock();
    host_error().len_i32()
}

/// Copies the last run's output into `dest` and returns how many bytes went,
/// under a lock held across the whole copy.
///
/// A negative `cap` asks only how many bytes there are and writes nothing, so a
/// host can size a read without guessing at [`MAX_OUTPUT_BYTES`]. Returns `-1`
/// for a null `dest` with a non-negative `cap`, which is the one way the copy
/// cannot be made.
///
/// The copy is whole: what comes out is one run's bytes, never a mixture of
/// two, because no write can happen while the lock is held. What it cannot
/// promise is that it is *this host's* run — the read is a second acquisition of
/// the lock after [`rb_run`] released it, and another host's run can be
/// scheduled in between. A host that runs one program at a time has no such
/// neighbour; one that does not wants [`rb_run_into`], which is the run and the
/// copy under one lock and so has no second moment in it.
///
/// # Safety
///
/// `dest` must point at `cap` writable bytes when `cap` is not negative.
#[no_mangle]
pub unsafe extern "C" fn rb_read_output(dest: *mut u8, cap: i32) -> i32 {
    if cap >= 0 && dest.is_null() {
        return -1;
    }
    let _run = run_lock();
    host_output().copy_out(dest, cap)
}

/// Copies the last run's error message into `dest` and returns how many bytes
/// went, under a lock held across the whole copy. A negative `cap` asks for the
/// length only. See [`rb_read_output`] for what this does and does not promise.
///
/// # Safety
///
/// `dest` must point at `cap` writable bytes when `cap` is not negative.
#[no_mangle]
pub unsafe extern "C" fn rb_read_error(dest: *mut u8, cap: i32) -> i32 {
    if cap >= 0 && dest.is_null() {
        return -1;
    }
    let _run = run_lock();
    host_error().copy_out(dest, cap)
}

/// Runs the program *and* copies both channels out, all under one lock.
///
/// The run-and-read form in one call, and the reason it exists is a schedule:
/// [`rb_run`] then [`rb_read_output`] is two acquisitions of [`run_lock`], and
/// another host's run landing between them means the host reads bytes that are
/// not its own. Two native threads doing exactly that was not a theoretical
/// worry — it is what
/// `edge_the_output_is_copied_out_under_the_lock_rather_than_read_through_a_pointer`
/// does, and it failed. Here there is no second moment for anything to land in.
///
/// `output_len` and `error_len` are in/out cells: the capacity in, the number of
/// bytes written out. A cell too small gets a short copy and the full length,
/// so the shortfall is something the host can see rather than something it has
/// to guess at. A null buffer with a capacity of zero is "do not copy this
/// one", which is how a host that only wants the error reads only the error.
///
/// Returns the status [`rb_run`] returns: `0` ran, `1` failed, `-1` refused.
///
/// # Safety
///
/// `ptr` must point at `len` initialised bytes (see [`rb_run`]); `output` and
/// `error` must each point at `*output_len` and `*error_len` writable bytes
/// respectively, or be null with their cell zero.
#[no_mangle]
pub unsafe extern "C" fn rb_run_into(
    ptr: *const u8,
    len: i32,
    output: *mut u8,
    output_len: *mut i32,
    error: *mut u8,
    error_len: *mut i32,
) -> i32 {
    if len < 0 || (len > 0 && ptr.is_null()) {
        return -1;
    }
    if (output.is_null() && !output_len.is_null() && unsafe { *output_len } != 0)
        || (error.is_null() && !error_len.is_null() && unsafe { *error_len } != 0)
    {
        return -1;
    }

    let _run = run_lock();
    let status = unsafe { write_channels(ptr, len) };

    if !output_len.is_null() {
        unsafe { *output_len = host_output().copy_out(output, *output_len) };
    }
    if !error_len.is_null() {
        unsafe { *error_len = host_error().copy_out(error, *error_len) };
    }
    status
}

/// The body of [`rb_run`]: write whatever the program produced into both host
/// channels and report whether it worked. The caller holds [`run_lock`].
///
/// Split out because [`rb_run_into`] does the same thing and then copies, and
/// two copies of this would be two chances to disagree.
unsafe fn write_channels(ptr: *const u8, len: i32) -> i32 {
    let slice = if len == 0 {
        &[][..]
    } else {
        // The host wrote these bytes through `rb_alloc` moments ago, and the
        // arena keeps them where they were. The borrow cannot be checked, which
        // is what `rb_run`'s safety contract is.
        std::slice::from_raw_parts(ptr, len as usize)
    };
    let source = match std::str::from_utf8(slice) {
        Ok(text) => text.to_owned(),
        Err(error) => {
            let message = format!("Source is not valid UTF-8 at byte {}", error.valid_up_to());
            host_error().set(&message);
            host_output().set("");
            return 1;
        }
    };

    let outcome = run_outcome(&source);

    // Both channels are written, always. Whatever the program printed before it
    // failed is still worth showing, and a host that found the output buffer
    // empty beside a message would read that as "this program printed nothing",
    // which is a different claim from "this program printed nothing I kept".
    //
    // `set` cannot report truncation here, and there is nothing for it to
    // report: this channel is allocated at exactly `MAX_OUTPUT_BYTES`, `emit`
    // refuses to collect past that many bytes, and `run_outcome` has already
    // turned an overflowing run into a failure with its bytes discarded. So the
    // two capacities are one number by construction, and a second overflow
    // message here would be a second wording for a case that cannot be reached
    // — a guard nobody can rely on is worse than none. Asserted, so every
    // `cargo test` run checks the invariant that makes it true.
    let complete = host_output().set(&outcome.printed);
    debug_assert!(
        complete,
        "captured output cannot exceed MAX_OUTPUT_BYTES: emit refuses to collect past it"
    );
    let failure = outcome.failure.unwrap_or_default();
    // A rendered error *can* be longer than its buffer — the source line it
    // quotes is any length a file can hold — and `set` says so by ending the
    // text with the truncation marker rather than by failing the run.
    host_error().set(&failure);

    // 0 for a run that worked, 1 for one that did not — and the error buffer
    // carries the difference, so an empty message is the success case.
    i32::from(!failure.is_empty())
}

/// The version string the page displays, by the same pointer-and-length pair as
/// every other byte the host reads. A `&'static str`, so the address is as
/// stable as the ones above.
#[no_mangle]
pub extern "C" fn rb_version_ptr() -> *const u8 {
    env!("CARGO_PKG_VERSION").as_ptr()
}

/// The byte length of [`rb_version_ptr`].
#[no_mangle]
pub extern "C" fn rb_version_len() -> i32 {
    i32::try_from(env!("CARGO_PKG_VERSION").len()).unwrap_or(i32::MAX)
}

/// The keyword list, by the same pair, out of a `OnceLock` that is never
/// replaced — so the address a host holds stays readable.
#[no_mangle]
pub extern "C" fn rb_keywords_ptr() -> *const u8 {
    keyword_list().as_ptr()
}

/// The byte length of [`rb_keywords_ptr`].
#[no_mangle]
pub extern "C" fn rb_keywords_len() -> i32 {
    i32::try_from(keyword_list().len()).unwrap_or(i32::MAX)
}

/// Records the host's wall clock, in seconds since the Unix epoch, so that
/// `time.now()` has something to read. `wasm32-unknown-unknown` has no clock of
/// its own and `SystemTime::now()` panics there, which would abort the module
/// rather than fail one run, so a playground that wants `time` calls this first
/// with `Date.now() / 1000`.
///
/// A negative or non-finite value is recorded as "no clock", which is what
/// `time.now()` then reports — the same answer a host that never called this
/// gets. It is not an error: the clock is not a parameter to a program, and
/// refusing it would give the host a failure mode it cannot cause on purpose.
#[no_mangle]
pub extern "C" fn rb_set_clock(seconds: f64) {
    #[cfg(target_arch = "wasm32")]
    crate::runtime::set_host_clock(seconds);
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Natively `time.now()` reads the system clock and ignores this. The
        // export exists in both builds so one set of bindings drives both.
        let _ = seconds;
    }
}
