// Classic FizzBuzz
for each n from 1 to 100
    if n is divisible by 15
        say "FizzBuzz"
    else if n is divisible by 3
        say "Fizz"
    else if n is divisible by 5
        say "Buzz"
    else
        say n
    end
end

// Alternative: using when statement (pattern matching)
for each n from 1 to 100
    when n mod 15
        case 0 then say "FizzBuzz"
        case 3 or 6 or 9 or 12 then say "Fizz"
        case 5 or 10 then say "Buzz"
        else say n
    end
end
