// Error Handling Examples

// Basic try/catch
try
    set data to might fail files.read("config.rb")
    say "Loaded: {data}"
catch error of FileError
    say "File not found. Using defaults."
    set data to "{}"
end

// Multiple catch blocks
try
    set result to might fail divide(10, 0)
    say "Result: {result}"
catch error of MathError
    say "Math error: {error message}"
catch error of FileError
    say "File error: {error message}"
catch error
    say "Unknown error: {error message}"
finally
    say "Cleanup always runs"
end

// Propagating errors
to might fail safe_divide(a, b)
    if b is equal to 0
        give back error "Cannot divide by zero"
    end
    give back a / b
end

to might fail calculate_ratio(list)
    set total to sum(list)
    set count to length of list
    if count is equal to 0
        give back error "Empty list"
    end
    give back total / count
end

// Using error in conditionals
set result to might fail safe_divide(10, 2)
if result is error
    say "Failed: {error message of result}"
else
    say "Success: {result}"
end

// Finally block cleanup
to might fail process_file(path)
    set file to nothing
    
    try
        set file to might fail files.open(path)
        set content to might fail files.read(file)
        give back content
    catch error
        give back error "Failed: {error message}"
    finally
        if file is not nothing
            might fail files.close(file)
        end
    end
end
