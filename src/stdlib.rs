use crate::value::Value;
use std::collections::HashMap;

pub fn builtins() -> HashMap<String, Value> {
    let mut globals = HashMap::new();
    
    // Math constants
    globals.insert("PI".to_string(), Value::Number(std::f64::consts::PI));
    globals.insert("E".to_string(), Value::Number(std::f64::consts::E));
    
    // Math functions
    globals.insert("abs".to_string(), Value::Builtin("abs".to_string()));
    globals.insert("floor".to_string(), Value::Builtin("floor".to_string()));
    globals.insert("ceil".to_string(), Value::Builtin("ceil".to_string()));
    globals.insert("round".to_string(), Value::Builtin("round".to_string()));
    globals.insert("sqrt".to_string(), Value::Builtin("sqrt".to_string()));
    globals.insert("pow".to_string(), Value::Builtin("pow".to_string()));
    globals.insert("sin".to_string(), Value::Builtin("sin".to_string()));
    globals.insert("cos".to_string(), Value::Builtin("cos".to_string()));
    globals.insert("tan".to_string(), Value::Builtin("tan".to_string()));
    globals.insert("log".to_string(), Value::Builtin("log".to_string()));
    globals.insert("exp".to_string(), Value::Builtin("exp".to_string()));
    
    // Text functions
    globals.insert("uppercase".to_string(), Value::Builtin("uppercase".to_string()));
    globals.insert("lowercase".to_string(), Value::Builtin("lowercase".to_string()));
    globals.insert("trim".to_string(), Value::Builtin("trim".to_string()));
    globals.insert("split".to_string(), Value::Builtin("split".to_string()));
    globals.insert("join".to_string(), Value::Builtin("join".to_string()));
    globals.insert("contains".to_string(), Value::Builtin("contains".to_string()));
    globals.insert("starts_with".to_string(), Value::Builtin("starts_with".to_string()));
    globals.insert("ends_with".to_string(), Value::Builtin("ends_with".to_string()));
    globals.insert("replace".to_string(), Value::Builtin("replace".to_string()));
    
    // List functions
    globals.insert("length".to_string(), Value::Builtin("length".to_string()));
    globals.insert("push".to_string(), Value::Builtin("push".to_string()));
    globals.insert("pop".to_string(), Value::Builtin("pop".to_string()));
    globals.insert("shift".to_string(), Value::Builtin("shift".to_string()));
    globals.insert("map".to_string(), Value::Builtin("map".to_string()));
    globals.insert("filter".to_string(), Value::Builtin("filter".to_string()));
    globals.insert("reduce".to_string(), Value::Builtin("reduce".to_string()));
    
    // Type checking
    globals.insert("is_number".to_string(), Value::Builtin("is_number".to_string()));
    globals.insert("is_text".to_string(), Value::Builtin("is_text".to_string()));
    globals.insert("is_list".to_string(), Value::Builtin("is_list".to_string()));
    globals.insert("is_record".to_string(), Value::Builtin("is_record".to_string()));
    
    // Conversion
    globals.insert("to_text".to_string(), Value::Builtin("to_text".to_string()));
    globals.insert("to_number".to_string(), Value::Builtin("to_number".to_string()));
    globals.insert("to_list".to_string(), Value::Builtin("to_list".to_string()));
    
    globals
}

pub fn builtin_function(name: &str, args: Vec<Value>) -> Option<Value> {
    match name {
        // Math functions
        "abs" => Some(args.first()?.abs()),
        "floor" => Some(args.first()?.floor()),
        "ceil" => Some(args.first()?.ceil()),
        "round" => Some(args.first()?.round()),
        "sqrt" => Some(args.first()?.sqrt()),
        
        // Text functions
        "uppercase" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.to_uppercase()))
            } else {
                None
            }
        },
        "lowercase" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.to_lowercase()))
            } else {
                None
            }
        },
        "trim" => {
            if let Value::Text(s) = args.first()? {
                Some(Value::Text(s.trim().to_string()))
            } else {
                None
            }
        },
        "length" => {
            match args.first()? {
                Value::Text(s) => Some(Value::Number(s.len() as f64)),
                Value::List(items) => Some(Value::Number(items.len() as f64)),
                _ => None,
            }
        },
        
        _ => None,
    }
}

impl Value {
    fn abs(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.abs()),
            _ => Value::Nothing,
        }
    }
    
    fn floor(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.floor()),
            _ => Value::Nothing,
        }
    }
    
    fn ceil(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.ceil()),
            _ => Value::Nothing,
        }
    }
    
    fn round(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.round()),
            _ => Value::Nothing,
        }
    }
    
    fn sqrt(&self) -> Value {
        match self {
            Value::Number(n) => Value::Number(n.sqrt()),
            _ => Value::Nothing,
        }
    }
}
