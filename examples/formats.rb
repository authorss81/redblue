// Formats Module Example
// Demonstrates JSON parsing

set json_str to "{\"name\": \"Alice\", \"age\": 30, \"active\": true}"
set obj to json.parse(json_str)
say "Parsed JSON:"
say obj

set name to obj.name
say "Name:"
say name

set json_arr to "[1, 2, 3]"
set arr to json.parse(json_arr)
say "Array:"
say arr

set result to json.stringify(obj)
say "Stringified:"
say result

say "Demo complete!"
