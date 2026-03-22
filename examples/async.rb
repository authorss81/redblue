// Async HTTP Request
import network
import formats

async to fetch_user(id)
    set url to "https://api.example.com/users/{id}"
    set response to wait network.get(url)
    set data to formats.parse_json(response body of response)
    give back data
end

async to fetch_all_data(user_id, post_id)
    parallel
        set user to wait fetch_user(user_id)
        set post to wait network.get("https://api.example.com/posts/{post_id}")
        set comments to wait network.get("https://api.example.com/posts/{post_id}/comments")
    until done
    
    give back record {
        user: user,
        post: formats.parse_json(post),
        comments: formats.parse_json(comments)
    }
end

// Usage
async to main()
    set data to wait fetch_all_data(1, 42)
    say "User: {data.user.name}"
    say "Post: {data.post.title}"
    say "Comments: {length of data.comments}"
end
