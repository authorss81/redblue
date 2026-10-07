to nest(n)
    if n is 0 then
        give back 0
    end
    give back n + nest(n - 1)
end
say nest(4)
