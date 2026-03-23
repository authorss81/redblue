// Files Module Example
// Demonstrates file reading, writing, and manipulation

// Write content to a file
files.write("output.txt", "Hello from Redblue!")

set content to files.read("output.txt")
say "File contents:"
say content

if files.exists("output.txt") then
    say "output.txt exists!"
end

set lines to files.lines("output.txt")
say "Number of lines:"
say length(lines)

files.append("output.txt", " - appended text")

set updated to files.read("output.txt")
say "Updated contents:"
say updated

files.copy("output.txt", "output_copy.txt")

if files.exists("output_copy.txt") then
    say "Copy created successfully!"
end

files.rename("output_copy.txt", "renamed.txt")

files.delete("output.txt")
files.delete("renamed.txt")

say "Done!"
