// Formats Module Example
// Demonstrates JSON and CSV parsing

// JSON parsing
set json_str to '{"name": "Alice", "age": 30, "active": yes}'
set obj to json.parse(json_str)
say "Parsed JSON object:"
say obj

// Access JSON properties
set name to obj.name
say "Name:"
say name

// Nested JSON
set nested to '{"user": {"email": "alice@example.com", "address": {"city": "NYC"}}}'
set parsed_nested to json.parse(nested)
say "City:"
say parsed_nested.user.address.city

// JSON array
set json_arr to '[1, 2, 3, "hello", {"key": "value"}]'
set arr to json.parse(json_arr)
say "JSON array:"
say arr
say "Third element:"
say arr at 2

// Stringify back to JSON
set back to json.stringify(obj)
say "Stringified:"
say back

// CSV parsing
set csv_data to "name,age,city
Alice,30,NYC
Bob,25,LA
Charlie,35,Chicago"

set rows to csv.parse(csv_data)
say "CSV rows:"
say rows

say "First row:"
say rows at 0

say "Second row name:"
say (rows at 1) at 0

say "Demo complete!"