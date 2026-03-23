use redblue::run_source;

#[test]
fn test_hello_world() {
    let result = run_source(r#"say "Hello, World!""#);
    assert!(result.is_ok());
}

#[test]
fn test_numbers() {
    let result = run_source(
        r#"
        set x to 10
        set y to 20
        say x
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn test_math() {
    let result = run_source(
        r#"
        set x to 10
        set y to 20
        set z to x + y
        say z
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn test_list() {
    let result = run_source(
        r#"
        set items to [1, 2, 3]
        say items
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn test_function() {
    let result = run_source(
        r#"
        to greet(name)
            say "Hello, {name}!"
        end
        greet("World")
    "#,
    );
    assert!(result.is_ok());
}
