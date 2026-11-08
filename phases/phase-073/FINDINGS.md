# Findings outside phase-073

## Early `give back` does not return from a function

**Evidence.** `src/interpreter.rs:2253`:

```rust
Statement::Return(expr) | Statement::GiveBack(expr) => match expr {
    Some(e) => self.evaluate(e),
    None => Ok(Value::Nothing),
},
```

The arm evaluates the value and yields it as the value of the statement. It does
not leave the enclosing block, so execution continues with the next statement in
the function body.

**Reproduction.**

```bash
cat > target/tmp/early-return.rb <<'RB'
to f(n)
    if n is less than 2 then
        give back 10
    end
    give back 20
end
say f(1)
say f(3)
RB
cargo run --quiet --bin rb -- run target/tmp/early-return.rb
```

Observed:

```
20
20
```

Expected per `SPEC.md` § "Return Values", which shows `give back` inside an
`if`/`else`:

```
10
20
```

This is not used by the phase-073 workloads (they are deliberately written with
a single final value to avoid depending on the broken behaviour), but it is a
real spec-drift bug for the auditor.
