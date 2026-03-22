// Time Module Example
// Demonstrates time functions

// Get current time
set now to time.now()
say "Current time:"
say now

// Get Unix timestamp
set timestamp to time.now().seconds
say "Unix timestamp:"
say timestamp

// Sleep for 1 second (uncomment to test)
say "Sleeping for 1 second..."
time.sleep(1)
say "Done sleeping!"

// Format time
set formatted to time.format(timestamp, "%Y-%m-%d %H:%M:%S")
say "Formatted time:"
say formatted

// Format current time
set now_formatted to time.format(time.now().seconds, "%A, %B %d, %Y")
say "Today's date:"
say now_formatted

// Parse Unix timestamp
set parsed to time.unix("2024-01-15 12:30:00")
say "Parsed timestamp:"
say parsed

say "Time module demo complete!"