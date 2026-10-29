# Phase 098 findings

- `lru_cache` now decorates a `to can` method as well as a plain `to`
  declaration. The compiled engine attaches the method's memory after
  `DEF_OBJECT`/`STORE`, through a hidden declaration call, so the `.rbc` format
  did not change.
- The capacity is fixed at declaration time; there is no runtime resize or
  inspection builtin. A caller that needs a different bound must declare a new
  function or method.
- `to can` methods that are inherited share the parent method value's cache, as
  they share the method value itself. That follows from a cache belonging to the
  function value, but a child that needs its own memory must declare its own
  decorated method.

## Resumed run note

The original finding (no chain/zip/enumerate/product/groupby helpers and no
memoisation) no longer reproduces: the prior attempt's implementation is in
`HEAD`, and the full suite exercises it. This run did not invent a replacement
for it. It closed the two remaining gaps in that work: `zip` now joins any
number of lists rather than exactly two, and the cross-engine iteration test
compares `Value`s as data rather than only their display text.
