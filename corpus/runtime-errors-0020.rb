set caught to no
try
    say 1 + "a"
catch error of Runtime failure
    say failure.kind
    say type_of(failure)
    set caught to yes
end
try
    say 1 / 0
catch error of Runtime
    say error.kind
finally
    say "cleanup"
end
say caught
