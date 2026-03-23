// Network Module Example
// Demonstrates HTTP GET and POST requests

// Note: These will only work with an internet connection

// HTTP GET request
say "Fetching example.com..."
set response to network.get("https://example.com")
say "Response received!"
say "First 200 characters:"
say substring(response, 0, 200)

// You can also parse JSON from HTTP
// set data to json.parse(network.get("https://api.example.com/data"))

// HTTP POST request
say "Sending POST request..."
set post_response to network.post("https://httpbin.org/post", "name=Alice&age=30")
say "POST response (first 300 chars):"
say substring(post_response, 0, 300)

say "Network module demo complete!"