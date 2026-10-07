to sum(xs)
    set total to 0
    for each x in xs
        set total to total + x
    end
    give back total
end
say sum([1, 2, 3])
