# Phase 098 findings

- `lru_cache` cannot decorate a `to can` object method: `modules/` users can
  only cache plain function declarations. SPEC.md line 1661 says a method call
  consults the same memory, which is true for a function value called through a
  method-shaped path but not for a method declared with `to can`. Supporting it
  would mean carrying the capacity on `Statement::Method` and in the compiled
  block, changing the `.rbc` format. Not in this phase's scope.
