object Outer
    object Inner
        has x default 1
    end
end
say type_of(Outer)
say type_of(Inner)
