// The random builtins, seeded.
//
// Every draw after `random_seed` is reproducible: this program prints the same
// thing every time it runs, in one process or in two, which is what lets it
// live in the differential corpus and in any golden-output test.
random_seed(2024)

// `random` is the whole-number draw. Both ends are included, so a range whose
// ends are equal has exactly one answer.
say "a die:"
say random(1, 6)
say "the only member of a range of one:"
say random(5, 5)

// `random_number` is the fractional draw: low end included, high end not.
say "between 0 and 1:"
say random_number(0, 1)
say "between -10 and -5:"
say random_number(-10, -5)

// `random_choice` takes one member of a list, and `random_shuffle` is a
// permutation of the list it was given.
set colours to ["red", "green", "blue"]
say "a colour:"
say random_choice(colours)
say "the same three, shuffled:"
say random_shuffle(colours)
