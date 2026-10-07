object Base
    has tag default "base"
end
object Middle extends Base
    has tag default "middle"
end
object Leaf extends Middle
end
say Leaf.tag
