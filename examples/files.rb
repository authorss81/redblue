// Files Module Example
// Demonstrates file reading, writing, and manipulation

// Write content to a file
files.write("output.txt", "Hello from Redblue!\nThis is a test file.")

// Read the entire file content
set content to files.read("output.txt")
say "File contents:"
say content

// Check if file exists
if files.exists("output.txt")
    say "output.txt exists!"
end

// Read file as lines
set lines to files.lines("output.txt")
say "Number of lines:"
say length(lines)

// Append more content
files.append("output.txt", "\nAppended line!")

// Read again to verify
set updated to files.read("output.txt")
say "Updated contents:"
say updated

// Copy the file
files.copy("output.txt", "output_copy.txt")

// Check the copy exists
if files.exists("output_copy.txt")
    say "Copy created successfully!"
end

// Rename the copy
files.rename("output_copy.txt", "renamed.txt")

// Clean up
files.delete("output.txt")
files.delete("renamed.txt")

say "Done!"
