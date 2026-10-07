to declare()
    object Inner
        has x default 1
    end
    give back 7
end
object Outer
    has tag default declare()
    has n default 2
end
say Outer.tag
say Outer.n
