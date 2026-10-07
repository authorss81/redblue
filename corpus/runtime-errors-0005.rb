set caught to no
try
    say no_such_function()
catch error
    set caught to yes
end
say caught
