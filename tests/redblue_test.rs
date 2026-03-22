use redblue::{run_source, Error};

#[test]
fn test_hello_world() {
    let result = run_source(r#"say "Hello, World!""#);
    assert!(result.is_ok());
}

#[test]
fn test_numbers() {
    let result = run_source(r#"
        set x to 10
        set y to 20
        say x
    "#);
    assert!(result.is_ok());
}

#[test]
fn test_math() {
    let result = run_source(r#"
        set x to 10
        set y to 20
        set z to x + y
        say z
    "#);
    assert!(result.is_ok());
}

#[test]
fn test_if_statement() {
    let result = run_source(r#"
        set x to 10
        if x is greater than 5
            say "big"
        else
            say "small"
        end
    "#);
    assert!(result.is_ok());
}

#[test]
fn test_repeat_loop() {
    let result = run_source(r#"
        set count to 0
        repeat 3 times
            add 1 to count
        end
        say count
    "#);
    assert!(result.is_ok());
}

#[test]
fn test_list() {
    let result = run_source(r#"
        set items to [1, 2, 3]
        say items
    "#);
    assert!(result.is_ok());
}

#[test]
fn test_yes_no() {
    let result = run_source(r#"
        set flag to yes
        if flag
            say "yes"
        end
    "#);
    assert!(result.is_ok());
}

#[test]
fn test_function() {
    let result = run_source(r#"
        to greet(name)
            say "Hello, {name}!"
        end
        greet("World")
    "#);
    assert!(result.is_ok());
}
