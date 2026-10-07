set caught to no
try
    try
        say 1 / 0
    catch inner
        set caught to yes
    end
catch error
    set caught to no
end
say caught
