# Redblue Performance History & Cumulative Speedup Registry

<!-- Maintained per Section 17 of docs/FUTURE_ROADMAP_AND_DEFERRED.md and BENCH.md -->

## 1. Architectural Purpose & The "Single File" Invariant

In high-performance language runtimes (such as V8, LuaJIT, PyPy, and Julia), performance optimizations are never evaluated in isolation. Every speedup phase must measure its concrete speedup against the **exact previous baseline on the same hardware**, proving:
1. **Measurable Speedup**: Quantitative wall-time or instruction-count improvement ($\Delta\%$, speedup multiplier).
2. **Bytecode / Semantic Invariance**: Proof that the faster compiler/runtime emits byte-identical bytecode or produces identical mathematical answers (`cmp == identical`, 100% test pass).
3. **Historical Regression Prevention**: A unified cumulative log preventing performance regressions from slipping in unnoticed during later refactors.

### The Single-File Rule
**Every speedup phase in Redblue must append its before/after benchmark numbers, hardware profile, and speedup multiplier to this file before its landing gate passes.**

---

## 2. Cumulative Speedup Ladder

| Phase | Optimization Name | Target Workload | Before (Baseline) | After (Optimized) | Speedup Multiplier | Delta (%) | Semantic Invariant Verified |
| :---: | :--- | :--- | :---: | :---: | :---: | :---: | :---: |
| **073** | *Continuous Benchmark Harness* | Standard suite (`fib`, `primes`, `concat`, `lex`) | Baseline set | Baseline set | $1.00\times$ | *Baseline Established* | 2,708 tests green, zero regression |
| **074** | *Chained Hash Interning Table* | Self-hosted compilation of `bootstrap/compiler.rb` | 147.34s | 70.07s | **$2.10\times$** | **+52.4% faster** | `cmp timed-old-out.rbc timed-new-out.rbc` $\rightarrow$ **identical** |
| **173** | *Inline Caching & Hidden Classes* | Property access on objects & records (`p.x + p.y`) | *Pending S17* | *Pending S17* | Target: $\ge 4.0\times$ | Target: $+75.0\%$ | Shape transition tree identical |
| **174** | *Monomorphic Unboxing* | Tight numeric loops & arithmetic (`sum`, `fib`, `primes`) | *Pending S17* | *Pending S17* | Target: $\ge 5.0\times$ | Target: $+80.0\%$ | Exact IEEE-754 / integer parity |
| **175** | *Escape Analysis & SRA* | Temporary record allocations & point math | *Pending S17* | *Pending S17* | Target: $\ge 3.0\times$ | Target: $+66.7\%$ | Zero heap allocation in loop |
| **176** | *SIMD Vectorization (AVX2/NEON)*| Matrix dot products, vector addition, array filters | *Pending S17* | *Pending S17* | Target: $\ge 8.0\times$ | Target: $+87.5\%$ | Exact 64-bit float precision |
| **177** | *Direct Threaded Dispatch VM* | Bytecode interpreter dispatch loop overhead | *Pending S17* | *Pending S17* | Target: $\ge 2.2\times$ | Target: $+55.0\%$ | Instruction trace equivalence |
| **178** | *Cranelift Native JIT Machine Codegen*| Full native execution of hot loops & functions | *Pending S17* | *Pending S17* | Target: $\ge 15.0\times$ | Target: $+93.3\%$ | Fixed-point bootstrap identical |

---

## 3. Detailed Phase Benchmarking Reports

### Phase 074: Chained Hash Interning Table
* **Landed Commit**: [`e54a690`](https://github.com/authorss81/redblue/commit/e54a690)
* **Optimization Mechanism**:
  Replaced the linear $O(N)$ `G_INTERN` scan in `to intern(raw)` with a 256-bucket chained hash table. Uses a deterministic byte hash reduced so every intermediate is exact in `f64`.
* **Hardware**: Ubuntu 24.04 LTS (Azure Hosted Compute, 4 vCPUs)
* **Measured Timings (Back-to-Back Control on Same Machine)**:
  ```bash
  $ target/debug/rb run target/tmp/compiler-old-timed.rb bootstrap/compiler.rb target/tmp/timed-old-out.rbc
  codegen_seconds 147.343627442

  $ target/debug/rb run target/tmp/compiler-new-timed.rb bootstrap/compiler.rb target/tmp/timed-new-out.rbc
  codegen_seconds 70.074412387

  $ cmp target/tmp/timed-old-out.rbc target/tmp/timed-new-out.rbc && echo identical
  identical
  ```
* **Performance Gain**:
  $$\text{Speedup} = \frac{147.34\text{s}}{70.07\text{s}} = 2.1026\times \quad (+52.44\%\text{ reduction in compile time})$$
* **Invariant**: Byte-identical bytecode output across both stages.

---

### Phase 073: Continuous Benchmark Suite
* **Landed Commit**: [`71ce4f8`](https://github.com/authorss81/redblue/commit/71ce4f8)
* **Optimization Mechanism**:
  Established the automated continuous regression testing framework (`tests/bench/mod.rs`, `tests/bench_test.rs`, and CLI `--compare`).
* **Hardware**: Intel(R) Xeon(R) 6973P-C (4 CPUs)
* **Baseline Measurements**:
  * `fibonacci(20)`: 0.971 ms
  * `primes(1000)`: 41.473 ms
  * `concat(200)`: 0.936 ms
  * `lex(200)`: 0.613 ms

---

## 4. Requirements for Future Speedup Phases (Phases 176–183)

When executing any optimization phase, the agent must adhere to the following protocol:
1. **Back-to-Back Comparison**: Run both the unmodified base compiler and the optimized compiler on the **exact same machine** within the same workflow run to eliminate cloud runner hardware variance.
2. **Bytecode Fixed-Point Check**: Assert that compiler optimizations emit byte-identical bytecode:
   ```bash
   cmp stage1_unoptimized.rbc stage2_optimized.rbc
   ```
3. **Append Entry to This File**: Update the table in Section 2 and append the reproduction snippet to Section 3 of `docs/PERFORMANCE_HISTORY.md`.
