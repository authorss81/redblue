object Outer
    has tag default "o"
    object Inner
        has x default 1
    end
    say "after inner"
end
say Outer.tag
