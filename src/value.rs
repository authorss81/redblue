use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nothing,
    Number(f64),
    Text(String),
    YesNo(bool),
    List(Vec<Value>),
    Record(HashMap<String, Value>),
    Object(String, HashMap<String, Value>),
    Function(String, Vec<String>),
    Builtin(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nothing => write!(f, "nothing"),
            Value::Number(n) => {
                if n.fract() == 0.0 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::Text(s) => write!(f, "{}", s),
            Value::YesNo(b) => write!(f, "{}", if *b { "yes" } else { "no" }),
            Value::List(items) => {
                let items: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Record(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect();
                write!(f, "{{{}}}", fields.join(", "))
            }
            Value::Function(name, _) => write!(f, "<function {}>", name),
            Value::Builtin(name) => write!(f, "<builtin {}>", name),
            Value::Object(_, _) => write!(f, "<object>"),
        }
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nothing => false,
            Value::YesNo(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::Text(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Record(_) => true,
            Value::Object(_, _) => true,
            Value::Function(_, _) => true,
            Value::Builtin(_) => true,
        }
    }
}
