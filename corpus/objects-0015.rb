object Base
    has tag default "base"
    object Child extends Base
        has extra default 1
    end
end
say Base.tag
