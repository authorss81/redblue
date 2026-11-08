// A probe for the intern table, kept out of `bootstrap/compiler.rb` so the
// compiler that is compiled by itself does not carry test-only code.
//
// This is a fragment, not a program: `tests/bootstrap_selfhost_test.rs` reads
// `compiler.rb`, removes its final `main()` call, appends this file and calls
// `intern_selftest()` (or `intern_selftest_type_error()`) instead. The probe
// therefore runs the same `intern`, `intern_hash` and bucket array the compile
// path runs, without adding a hidden mode to the compiler's own `main`.

to intern_selftest()
    set intern_selftest_start to length(G_INTERN)

    // One name, a thousand times: the table grows by one, not by a thousand.
    set intern_selftest_repeated to bytes.from_text("repeated")
    set intern_selftest_first to intern(intern_selftest_repeated)
    set intern_selftest_i to 0
    while intern_selftest_i < 1000
        intern(intern_selftest_repeated)
        set intern_selftest_i to intern_selftest_i + 1
    end
    say "repeat_growth " + to_text(length(G_INTERN) - intern_selftest_start)
    intern(bytes.from_text("other"))
    say "other_growth " + to_text(length(G_INTERN) - intern_selftest_start)

    // The empty name is a name, not a missing one.
    set intern_selftest_empty_start to length(G_INTERN)
    set intern_selftest_empty_one to intern([])
    set intern_selftest_empty_two to intern([])
    say "empty_same " + intern_selftest_yes(intern_selftest_empty_one is intern_selftest_empty_two)
    say "empty_growth " + to_text(length(G_INTERN) - intern_selftest_empty_start)

    // A name far longer than a bucket count, and a variant that differs only in
    // its last byte. A hash that truncated the name would give them one index.
    set intern_selftest_long_a to []
    set intern_selftest_long_b to []
    set intern_selftest_i to 0
    while intern_selftest_i < 10000
        append("intern_selftest_long_a", 120)
        append("intern_selftest_long_b", 120)
        set intern_selftest_i to intern_selftest_i + 1
    end
    append("intern_selftest_long_b", 121)
    set intern_selftest_long_start to length(G_INTERN)
    set intern_selftest_long_one to intern(intern_selftest_long_a)
    set intern_selftest_long_two to intern(intern_selftest_long_a)
    set intern_selftest_long_three to intern(intern_selftest_long_b)
    say "long_same " + intern_selftest_yes(intern_selftest_long_one is intern_selftest_long_two)
    say "long_distinct " + intern_selftest_yes(intern_selftest_long_one is not intern_selftest_long_three)
    say "long_growth " + to_text(length(G_INTERN) - intern_selftest_long_start)

    // Two distinct names engineered to select the same bucket: keep the first
    // candidate, then walk a sequence of distinct names until one hashes to its
    // bucket. Both then have to be in the bucket at once and each has to answer
    // with its own index.
    set intern_selftest_a to bytes.from_text("collision-0")
    set intern_selftest_a_bucket to intern_hash(intern_selftest_a) mod INTERN_BUCKETS
    set intern_selftest_b to []
    set intern_selftest_found to no
    set intern_selftest_i to 1
    while intern_selftest_i < 20000
        if intern_selftest_found is no then
            set intern_selftest_b to bytes.from_text("collision-" + to_text(intern_selftest_i))
            if (intern_hash(intern_selftest_b) mod INTERN_BUCKETS) is intern_selftest_a_bucket then
                set intern_selftest_found to yes
            end
        end
        set intern_selftest_i to intern_selftest_i + 1
    end
    set intern_selftest_a_index to intern(intern_selftest_a)
    set intern_selftest_b_index to intern(intern_selftest_b)
    say "collision_found " + intern_selftest_yes(intern_selftest_found)
    say "collision_distinct " + intern_selftest_yes(intern_selftest_a_index is not intern_selftest_b_index)
    say "collision_a_bucket " + to_text(intern_selftest_a_bucket)
    say "collision_b_bucket " + to_text(intern_hash(intern_selftest_b) mod INTERN_BUCKETS)
    say "collision_bucket " + to_text(intern_selftest_a_bucket)
    say "collision_a_index " + to_text(intern_selftest_a_index)
    say "collision_b_index " + to_text(intern_selftest_b_index)
    say "collision_a_lookup " + to_text(intern(intern_selftest_a))
    say "collision_b_lookup " + to_text(intern(intern_selftest_b))

    // A few hundred ordinary names have to occupy many different buckets. A
    // hash that returned a constant would put every name in one bucket and the
    // assertions above would still hold, because they only check deduplication.
    // Counting non-empty buckets and the longest chain is what notices a lookup
    // that is still a scan of every name.
    set intern_selftest_spread_start to length(G_INTERN)
    set intern_selftest_spread_i to 0
    while intern_selftest_spread_i < 300
        intern(bytes.from_text("spread-" + to_text(intern_selftest_spread_i)))
        set intern_selftest_spread_i to intern_selftest_spread_i + 1
    end
    say "spread_growth " + to_text(length(G_INTERN) - intern_selftest_spread_start)

    set intern_selftest_nonempty to 0
    set intern_selftest_max_chain to 0
    set intern_selftest_i to 0
    while intern_selftest_i < INTERN_BUCKETS
        set intern_selftest_chain to length(G_INTERN_BUCKETS[intern_selftest_i])
        if intern_selftest_chain > 0 then
            set intern_selftest_nonempty to intern_selftest_nonempty + 1
        end
        if intern_selftest_chain > intern_selftest_max_chain then
            set intern_selftest_max_chain to intern_selftest_chain
        end
        set intern_selftest_i to intern_selftest_i + 1
    end
    say "distinct_buckets " + to_text(intern_selftest_nonempty)
    say "max_chain " + to_text(intern_selftest_max_chain)

    // Two ordinary names, each selected by the hash rather than engineered to
    // collide, must still choose different buckets. This is the direct check
    // that the hash discriminates at all; the bucket count above is the one
    // that says it discriminates across hundreds of names.
    set intern_selftest_ordinary_a to bytes.from_text("ordinary-a")
    set intern_selftest_ordinary_b to bytes.from_text("ordinary-b")
    intern(intern_selftest_ordinary_a)
    intern(intern_selftest_ordinary_b)
    say "ordinary_a_bucket " + to_text(intern_hash(intern_selftest_ordinary_a) mod INTERN_BUCKETS)
    say "ordinary_b_bucket " + to_text(intern_hash(intern_selftest_ordinary_b) mod INTERN_BUCKETS)
    give back 0
end

// The same words the compiler prints for a yes/no.
to intern_selftest_yes(flag)
    set intern_selftest_yes_text to "no"
    if flag is yes then
        set intern_selftest_yes_text to "yes"
    end
    give back intern_selftest_yes_text
end

// A name that is not a byte list must fail at the table, not be accepted as
// some other key. The test runs this instead of `intern_selftest`, so the
// failure is the compiler's own runtime error and not a panic.
to intern_selftest_type_error()
    intern(42)
    give back 0
end
