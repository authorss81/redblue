pub mod text;
pub mod math;
pub mod files;
pub mod network;
pub mod formats;
pub mod list;
pub mod console;

pub use text::*;
pub use math::*;
pub use files::*;
pub use network::*;
pub use formats::*;
pub use list::*;
pub use console::*;

use crate::value::Value;
use crate::error::Result;
use std::collections::HashMap;

pub fn all_modules() -> HashMap<String, Value> {
    let mut modules = HashMap::new();
    
    modules.insert("text".to_string(), Value::Builtin("module: text".to_string()));
    modules.insert("math".to_string(), Value::Builtin("module: math".to_string()));
    modules.insert("files".to_string(), Value::Builtin("module: files".to_string()));
    modules.insert("network".to_string(), Value::Builtin("module: network".to_string()));
    modules.insert("formats".to_string(), Value::Builtin("module: formats".to_string()));
    modules.insert("list".to_string(), Value::Builtin("module: list".to_string()));
    modules.insert("console".to_string(), Value::Builtin("module: console".to_string()));
    
    modules
}

pub fn call_module_function(module: &str, function: &str, args: Vec<Value>) -> Result<Value> {
    match module {
        "text" => text::call(function, args),
        "math" => math::call(function, args),
        "files" => files::call(function, args),
        "network" => network::call(function, args),
        "formats" => formats::call(function, args),
        "list" => list::call(function, args),
        "console" => console::call(function, args),
        _ => Err(crate::error::Error::Runtime(format!("Unknown module '{}'", module))),
    }
}
