use crate::testing::{TestResults, TestError};
use crate::value::Value;

pub trait Assertion<T> {
    fn assert(self, actual: T) -> Result<(), TestAssertionError>;
}

pub struct Expected<T>(pub T);

pub struct TestAssertionError {
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

impl std::fmt::Debug for TestAssertionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(expected) = &self.expected {
            writeln!(f, "Expected: {}", expected)?;
        }
        if let Some(actual) = &self.actual {
            writeln!(f, "Actual: {}", actual)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for TestAssertionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub fn assert_that<T: std::fmt::Debug + PartialEq + Clone>(actual: T) -> AssertThat<T> {
    AssertThat { actual }
}

pub struct AssertThat<T> {
    actual: T,
}

impl<T: std::fmt::Debug + PartialEq + Clone> AssertThat<T> {
    pub fn is_equal_to(&self, expected: T) -> Result<(), TestAssertionError> {
        if self.actual == expected {
            Ok(())
        } else {
            Err(TestAssertionError {
                message: format!("{:?} should equal {:?}", self.actual, expected),
                expected: Some(format!("{:?}", expected)),
                actual: Some(format!("{:?}", self.actual)),
            })
        }
    }
    
    pub fn is_not_equal_to(&self, not_expected: T) -> Result<(), TestAssertionError> {
        if self.actual != not_expected {
            Ok(())
        } else {
            Err(TestAssertionError {
                message: format!("{:?} should NOT equal {:?}", self.actual, not_expected),
                expected: Some(format!("NOT {:?}", not_expected)),
                actual: Some(format!("{:?}", self.actual)),
            })
        }
    }
    
    pub fn is_same_as(&self, expected: &T) -> Result<(), TestAssertionError>
    where T: PartialEq {
        if std::ptr::eq(self.actual, expected) {
            Ok(())
        } else {
            Err(TestAssertionError {
                message: "Objects should be the same".to_string(),
                expected: None,
                actual: None,
            })
        }
    }
    
    pub fn is_none(&self) -> Result<(), TestAssertionError>
    where T: std::fmt::Debug {
        match &self.actual {
            None => Ok(()),
            Some(v) => Err(TestAssertionError {
                message: format!("Expected None but got Some({:?})", v),
                expected: Some("None".to_string()),
                actual: Some(format!("Some({:?})", v)),
            }),
        }
    }
    
    pub fn is_some(&self) -> Result<(), TestAssertionError>
    where T: std::fmt::Debug {
        match &self.actual {
            Some(_) => Ok(()),
            None => Err(TestAssertionError {
                message: "Expected Some but got None".to_string(),
                expected: Some("Some".to_string()),
                actual: Some("None".to_string()),
            }),
        }
    }
    
    pub fn contains(&self, item: &T) -> Result<(), TestAssertionError>
    where T: PartialEq + std::fmt::Debug {
        if self.actual == *item {
            Err(TestAssertionError {
                message: format!("{:?} should contain {:?}", self.actual, item),
                expected: Some(format!("Contains {:?}", item)),
                actual: Some(format!("{:?}", self.actual)),
            })
        } else {
            Ok(())
        }
    }
}

pub fn assert_values_equal(expected: &Value, actual: &Value) -> Result<(), TestAssertionError> {
    if expected == actual {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Values not equal: {:?} vs {:?}", expected, actual),
            expected: Some(format!("{:?}", expected)),
            actual: Some(format!("{:?}", actual)),
        })
    }
}

pub fn assert_value_is_number(value: &Value) -> Result<(), TestAssertionError> {
    match value {
        Value::Number(_) => Ok(()),
        _ => Err(TestAssertionError {
            message: format!("Expected number but got {:?}", value),
            expected: Some("number".to_string()),
            actual: Some(format!("{:?}", value)),
        }),
    }
}

pub fn assert_value_is_text(value: &Value) -> Result<(), TestAssertionError> {
    match value {
        Value::Text(_) => Ok(()),
        _ => Err(TestAssertionError {
            message: format!("Expected text but got {:?}", value),
            expected: Some("text".to_string()),
            actual: Some(format!("{:?}", value)),
        }),
    }
}

pub fn assert_value_is_list(value: &Value) -> Result<(), TestAssertionError> {
    match value {
        Value::List(_) => Ok(()),
        _ => Err(TestAssertionError {
            message: format!("Expected list but got {:?}", value),
            expected: Some("list".to_string()),
            actual: Some(format!("{:?}", value)),
        }),
    }
}

pub fn assert_value_is_yes_no(value: &Value) -> Result<(), TestAssertionError> {
    match value {
        Value::YesNo(_) => Ok(()),
        _ => Err(TestAssertionError {
            message: format!("Expected yes/no but got {:?}", value),
            expected: Some("yes/no".to_string()),
            actual: Some(format!("{:?}", value)),
        }),
    }
}

pub fn assert_value_is_record(value: &Value) -> Result<(), TestAssertionError> {
    match value {
        Value::Record(_) => Ok(()),
        _ => Err(TestAssertionError {
            message: format!("Expected record but got {:?}", value),
            expected: Some("record".to_string()),
            actual: Some(format!("{:?}", value)),
        }),
    }
}

pub fn assert_list_length(list: &Value, expected: usize) -> Result<(), TestAssertionError> {
    match list {
        Value::List(items) => {
            if items.len() == expected {
                Ok(())
            } else {
                Err(TestAssertionError {
                    message: format!("Expected list length {} but got {}", expected, items.len()),
                    expected: Some(expected.to_string()),
                    actual: Some(items.len().to_string()),
                })
            }
        },
        _ => Err(TestAssertionError {
            message: format!("Expected list but got {:?}", list),
            expected: Some("list".to_string()),
            actual: Some(format!("{:?}", list)),
        }),
    }
}

pub fn assert_text_contains(text: &str, substring: &str) -> Result<(), TestAssertionError> {
    if text.contains(substring) {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Text '{}' does not contain '{}'", text, substring),
            expected: Some(format!("Contains '{}'", substring)),
            actual: Some(text.to_string()),
        })
    }
}

pub fn assert_text_matches(text: &str, pattern: &str) -> Result<(), TestAssertionError> {
    if regex::Regex::new(pattern)
        .map(|re| re.is_match(text))
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Text '{}' does not match pattern '{}'", text, pattern),
            expected: Some(format!("Matches '{}'", pattern)),
            actual: Some(text.to_string()),
        })
    }
}

pub fn assert_number_in_range(value: f64, min: f64, max: f64) -> Result<(), TestAssertionError> {
    if value >= min && value <= max {
        Ok(())
    } else {
        Err(TestAssertionError {
            message: format!("Number {} is not in range [{}, {}]", value, min, max),
            expected: Some(format!("[{}, {}]", min, max)),
            actual: Some(value.to_string()),
        })
    }
}

pub fn assert_throws<F>(mut f: F) -> Result<(), TestAssertionError>
where
    F: FnMut(),
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut f));
    
    match result {
        Err(_) => Ok(()),
        Ok(_) => Err(TestAssertionError {
            message: "Expected to throw but did not".to_string(),
            expected: Some("panic".to_string()),
            actual: Some("no panic".to_string()),
        }),
    }
}
