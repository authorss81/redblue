// Classic FizzBuzz using repeat loop
set n to 1
repeat 15 times
    if n % 15 is 0 then
        say "FizzBuzz"
    else
        if n % 3 is 0 then
            say "Fizz"
        else
            if n % 5 is 0 then
                say "Buzz"
            else
                say n
            end
        end
    end
    set n to n + 1
end

// Using for each with a list
set numbers to [1, 2, 3, 4, 5]
for each num in numbers
    say num
end
