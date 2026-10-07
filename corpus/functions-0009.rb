to count_down(n)
    if n is 0 then
        give back 0
    end
    give back n + count_down(n - 1)
end
say count_down(3)
