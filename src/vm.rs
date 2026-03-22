use crate::error::{Error, Result};
use crate::parser::{Program, Statement, Expr, BinaryOp, UnaryOp, ImportItem};
use crate::value::Value;
use crate::stdlib;
use crate::lexer;
use crate::parser as redblue_parser;
use std::collections::HashMap;
use std::path::Path;

pub struct Vm {
    globals: HashMap<String, Value>,
    locals: Vec<HashMap<String, Value>>,
    functions: HashMap<String, Vec<String>>,
    output: Vec<String>,
    modules: HashMap<String, Program>,
}

impl Vm {
    pub fn new() -> Self {
        let mut globals = stdlib::builtins();
        Self {
            globals,
            locals: vec![HashMap::new()],
            functions: HashMap::new(),
            output: Vec::new(),
            modules: HashMap::new(),
        }
    }
    
    fn load_module(&mut self, path: &str) -> Result<()> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| Error::Io(format!("Cannot load module '{}': {}", path, e)))?;
        
        let tokens = lexer::tokenize(&source)?;
        let ast = redblue_parser::Parser::new(tokens).parse()?;
        
        for stmt in &ast.statements {
            match stmt {
                Statement::Set { name, value } => {
                    let val = self.evaluate(value)?;
                    self.globals.insert(name.clone(), val);
                },
                Statement::Function { name, params, body } => {
                    self.functions.insert(name.clone(), params.clone());
                },
                _ => {}
            }
        }
        
        Ok(())
    }
    
    pub fn run(&mut self, program: &Program) -> Result<Value> {
        let mut result = Value::Nothing;
        
        for statement in &program.statements {
            result = self.execute_statement(statement)?;
        }
        
        // Print collected output
        for line in &self.output {
            println!("{}", line);
        }
        
        Ok(result)
    }
    
    fn push_scope(&mut self) {
        self.locals.push(HashMap::new());
    }
    
    fn pop_scope(&mut self) {
        self.locals.pop();
    }
    
    fn get_var(&self, name: &str) -> Option<Value> {
        // Check local scopes first
        for scope in self.locals.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        // Check globals
        self.globals.get(name).cloned()
    }
    
    fn set_var(&mut self, name: &str, value: Value) {
        if let Some(scope) = self.locals.last_mut() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return;
            }
        }
        self.globals.insert(name.to_string(), value);
    }
    
    fn declare(&mut self, name: &str) {
        if let Some(scope) = self.locals.last_mut() {
            scope.insert(name.to_string(), Value::Nothing);
        }
    }
    
    fn execute_statement(&mut self, stmt: &Statement) -> Result<Value> {
        match stmt {
            Statement::Say(expr) => {
                let value = self.evaluate(expr)?;
                self.output.push(value.to_string());
                Ok(Value::Nothing)
            },
            Statement::Print(expr) => {
                let value = self.evaluate(expr)?;
                print!("{}", value);
                Ok(Value::Nothing)
            },
            Statement::Set { name, value } => {
                let val = self.evaluate(value)?;
                self.set_var(name, val);
                Ok(Value::Nothing)
            },
            Statement::SetProperty { object, property, value } => {
                let val = self.evaluate(value)?;
                if let Some(Value::Record(mut fields)) = self.get_var(object) {
                    fields.insert(property.clone(), val);
                    self.set_var(object, Value::Record(fields));
                }
                Ok(Value::Nothing)
            },
            Statement::If { condition, then_branch, else_branch } => {
                let cond = self.evaluate(condition)?;
                if cond.is_truthy() {
                    for stmt in then_branch {
                        self.execute_statement(stmt)?;
                    }
                } else {
                    for stmt in else_branch {
                        self.execute_statement(stmt)?;
                    }
                }
                Ok(Value::Nothing)
            },
            Statement::ForEach { variable, iterable, body } => {
                let iterable_value = self.evaluate(iterable)?;
                if let Value::List(items) = iterable_value {
                    for item in items {
                        self.push_scope();
                        self.declare(variable);
                        self.set_var(variable, item);
                        for stmt in body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
                    }
                }
                Ok(Value::Nothing)
            },
            Statement::ForRange { variable, start, end, step, body } => {
                let start_val = self.evaluate(start)?;
                let end_val = self.evaluate(end)?;
                let step_val = match step {
                    Some(s) => self.evaluate(s)?,
                    None => Value::Number(1.0),
                };
                
                if let (Value::Number(start), Value::Number(end), Value::Number(step)) = (start_val, end_val, step_val) {
                    let mut i = start;
                    while i <= end {
                        self.push_scope();
                        self.declare(variable);
                        self.set_var(variable, Value::Number(i));
                        for stmt in body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
                        i += step;
                    }
                }
                Ok(Value::Nothing)
            },
            Statement::Repeat { count, body } => {
                let count_val = self.evaluate(count)?;
                if let Value::Number(n) = count_val {
                    for _ in 0..(n as i64) {
                        self.push_scope();
                        for stmt in body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
                    }
                }
                Ok(Value::Nothing)
            },
            Statement::While { condition, body } => {
                while self.evaluate(condition)?.is_truthy() {
                    self.push_scope();
                    for stmt in body {
                        self.execute_statement(stmt)?;
                    }
                    self.pop_scope();
                }
                Ok(Value::Nothing)
            },
            Statement::Break => {
                // TODO: Implement proper control flow
                Ok(Value::Nothing)
            },
            Statement::Skip => {
                // TODO: Implement proper control flow
                Ok(Value::Nothing)
            },
            Statement::Return(expr) | Statement::GiveBack(expr) => {
                match expr {
                    Some(e) => self.evaluate(e),
                    None => Ok(Value::Nothing),
                }
            },
            Statement::Function { name, params, body } => {
                self.declare(name);
                self.set_var(name, Value::Function(name.clone(), params.clone()));
                self.functions.insert(name.clone(), params.clone());
                
                // Store function body (simplified - just store params)
                // Full implementation would store AST
                Ok(Value::Nothing)
            },
            Statement::Method { name, params, body: _ } => {
                // Method implementation
                Ok(Value::Nothing)
            },
            Statement::Object { name, extends: _, body: _ } => {
                // Object implementation
                let record = Value::Record(HashMap::new());
                self.set_var(name, record);
                Ok(Value::Nothing)
            },
            Statement::Try { body, catch_body, finally_body, .. } => {
                let result = self.execute_statements(body);
                
                if result.is_err() {
                    if let Some(catch_var) = catch_body.as_ref() {
                        self.push_scope();
                        self.declare(catch_var);
                        self.set_var(catch_var, Value::Text("error".to_string()));
                        for stmt in catch_body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
                    }
                }
                
                for stmt in finally_body {
                    self.execute_statement(stmt)?;
                }
                
                Ok(Value::Nothing)
            },
            Statement::Import(items) => {
                for item in items {
                    let module_path = format!("modules/{}.rb", item.name);
                    let search_paths = vec![
                        Path::new(&module_path),
                        Path::new(&format!("./modules/{}", item.name)),
                        Path::new(&item.name),
                    ];
                    
                    let mut loaded = false;
                    for path in search_paths {
                        if path.exists() {
                            self.load_module(path.to_str().unwrap())?;
                            loaded = true;
                            break;
                        }
                    }
                    
                    if !loaded {
                        return Err(Error::Runtime(format!("Cannot find module '{}'", item.name)));
                    }
                    
                    let target_name = item.alias.as_ref().unwrap_or(&item.name);
                    self.globals.insert(target_name.clone(), Value::Nothing);
                }
                Ok(Value::Nothing)
            },
            Statement::Test { name, body } => {
                // Execute test body
                for stmt in body {
                    self.execute_statement(stmt)?;
                }
                Ok(Value::Nothing)
            },
            Statement::Expr(expr) => {
                self.evaluate(expr)
            },
        }
    }
    
    fn execute_statements(&self, statements: &[Statement]) -> Result<Value> {
        let mut result = Value::Nothing;
        for stmt in statements {
            result = self.execute_statement(stmt)?;
        }
        Ok(result)
    }
    
    fn evaluate(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::Text(s) => Ok(Value::Text(s.clone())),
            Expr::YesNo(b) => Ok(Value::YesNo(*b)),
            Expr::Nothing => Ok(Value::Nothing),
            Expr::Variable(name) => {
                self.get_var(name)
                    .ok_or_else(|| Error::Runtime(format!("Unknown variable '{}'", name)))
            },
            Expr::Binary { op, left, right } => {
                let l = self.evaluate(left)?;
                let r = self.evaluate(right)?;
                self.binary_op(op, l, r)
            },
            Expr::Unary { op, expr } => {
                let v = self.evaluate(expr)?;
                self.unary_op(op, v)
            },
            Expr::Call { name, args } => {
                self.call(name, args)
            },
            Expr::Property { object, property } => {
                let obj = self.evaluate(object)?;
                if let Value::Record(fields) = obj {
                    Ok(fields.get(property).cloned().unwrap_or(Value::Nothing))
                } else {
                    Err(Error::Runtime(format!("Cannot access property on non-object")))
                }
            },
            Expr::Index { object, index } => {
                let obj = self.evaluate(object)?;
                let idx = self.evaluate(index)?;
                if let Value::List(items) = obj {
                    if let Value::Number(n) = idx {
                        let i = if n < 0 { items.len() as i64 + n as i64 } else { n as i64 };
                        Ok(items.get(i as usize).cloned().unwrap_or(Value::Nothing))
                    } else {
                        Err(Error::Runtime("Index must be a number".to_string()))
                    }
                } else {
                    Err(Error::Runtime("Cannot index non-list".to_string()))
                }
            },
            Expr::InterpolatedText(parts) => {
                let mut result = String::new();
                for part in parts {
                    result.push_str(&self.evaluate(part)?.to_string());
                }
                Ok(Value::Text(result))
            },
            Expr::List(items) => {
                let values: Result<Vec<Value>> = items.iter().map(|i| self.evaluate(i)).collect();
                Ok(Value::List(values?))
            },
            Expr::Record(fields) => {
                let mut record = HashMap::new();
                for (key, value) in fields {
                    record.insert(key.clone(), self.evaluate(value)?);
                }
                Ok(Value::Record(record))
            },
        }
    }
    
    fn binary_op(&mut self, op: &BinaryOp, left: Value, right: Value) -> Result<Value> {
        match op {
            BinaryOp::Add => {
                match (left, right) {
                    (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                    (Value::Text(a), Value::Text(b)) => Ok(Value::Text(format!("{}{}", a, b))),
                    _ => Err(Error::Runtime("Cannot add non-numbers".to_string())),
                }
            },
            BinaryOp::Sub => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::Number(a - b))
                } else {
                    Err(Error::Runtime("Cannot subtract non-numbers".to_string()))
                }
            },
            BinaryOp::Mul => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::Number(a * b))
                } else {
                    Err(Error::Runtime("Cannot multiply non-numbers".to_string()))
                }
            },
            BinaryOp::Div => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    if b == 0.0 {
                        Err(Error::Runtime("Division by zero".to_string()))
                    } else {
                        Ok(Value::Number(a / b))
                    }
                } else {
                    Err(Error::Runtime("Cannot divide non-numbers".to_string()))
                }
            },
            BinaryOp::Mod => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::Number(a % b))
                } else {
                    Err(Error::Runtime("Cannot modulo non-numbers".to_string()))
                }
            },
            BinaryOp::Equal => Ok(Value::YesNo(left == right)),
            BinaryOp::NotEqual => Ok(Value::YesNo(left != right)),
            BinaryOp::Less => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::YesNo(a < b))
                } else {
                    Err(Error::Runtime("Cannot compare non-numbers".to_string()))
                }
            },
            BinaryOp::LessEqual => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::YesNo(a <= b))
                } else {
                    Err(Error::Runtime("Cannot compare non-numbers".to_string()))
                }
            },
            BinaryOp::Greater => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::YesNo(a > b))
                } else {
                    Err(Error::Runtime("Cannot compare non-numbers".to_string()))
                }
            },
            BinaryOp::GreaterEqual => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::YesNo(a >= b))
                } else {
                    Err(Error::Runtime("Cannot compare non-numbers".to_string()))
                }
            },
            BinaryOp::And => Ok(Value::YesNo(left.is_truthy() && right.is_truthy())),
            BinaryOp::Or => Ok(Value::YesNo(left.is_truthy() || right.is_truthy())),
            BinaryOp::In => {
                if let Value::List(items) = right {
                    Ok(Value::YesNo(items.contains(&left)))
                } else {
                    Err(Error::Runtime("Right side of 'in' must be a list".to_string()))
                }
            },
        }
    }
    
    fn unary_op(&self, op: &UnaryOp, value: Value) -> Result<Value> {
        match op {
            UnaryOp::Neg => {
                if let Value::Number(n) = value {
                    Ok(Value::Number(-n))
                } else {
                    Err(Error::Runtime("Cannot negate non-number".to_string()))
                }
            },
            UnaryOp::Not => Ok(Value::YesNo(!value.is_truthy())),
        }
    }
    
    fn call(&mut self, name: &str, args: &[Expr]) -> Result<Value> {
        let arg_values: Result<Vec<Value>> = args.iter().map(|a| self.evaluate(a)).collect();
        let args = arg_values?;
        
        // Check for built-in functions
        match name {
            "say" => {
                if let Some(arg) = args.first() {
                    println!("{}", arg);
                    Ok(Value::Nothing)
                } else {
                    Err(Error::Runtime("say requires an argument".to_string()))
                }
            },
            "length" | "len" => {
                if let Some(Value::List(items)) = args.first().cloned() {
                    Ok(Value::Number(items.len() as f64))
                } else if let Some(Value::Text(s)) = args.first().cloned() {
                    Ok(Value::Number(s.len() as f64))
                } else {
                    Err(Error::Runtime("length requires a list or text".to_string()))
                }
            },
            "input" | "ask" => {
                let mut input = String::new();
                if let Some(prompt) = args.first() {
                    print!("{}", prompt);
                }
                std::io::stdin().read_line(&mut input).map_err(|e| Error::Runtime(e.to_string()))?;
                input.pop(); // Remove newline
                Ok(Value::Text(input))
            },
            "random" => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                Ok(Value::Number((now.as_nanos() % 1000) as f64))
            },
            // Files module
            "files_read" => {
                let path = match args.first() {
                    Some(Value::Text(p)) => p,
                    _ => return Err(Error::Runtime("files.read requires a text path".to_string())),
                };
                std::fs::read_to_string(path)
                    .map(Value::Text)
                    .map_err(|e| Error::Io(format!("Failed to read '{}': {}", path, e)))
            },
            "files_write" => {
                let (path, content) = match (&args.get(0), args.get(1)) {
                    (Some(Value::Text(p)), Some(Value::Text(c))) => (p, c),
                    _ => return Err(Error::Runtime("files.write requires two text arguments".to_string())),
                };
                std::fs::write(path, content)
                    .map_err(|e| Error::Io(format!("Failed to write '{}': {}", path, e)))?;
                Ok(Value::Nothing)
            },
            "files_append" => {
                let (path, content) = match (&args.get(0), args.get(1)) {
                    (Some(Value::Text(p)), Some(Value::Text(c))) => (p, c),
                    _ => return Err(Error::Runtime("files.append requires two text arguments".to_string())),
                };
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .and_then(|mut f| std::io::Write::write_all(&mut f, content.as_bytes()))
                    .map_err(|e| Error::Io(format!("Failed to append to '{}': {}", path, e)))?;
                Ok(Value::Nothing)
            },
            "files_exists" => {
                let path = match args.first() {
                    Some(Value::Text(p)) => p,
                    _ => return Err(Error::Runtime("files.exists requires a text path".to_string())),
                };
                Ok(Value::YesNo(std::path::Path::new(path).exists()))
            },
            "files_lines" => {
                let path = match args.first() {
                    Some(Value::Text(p)) => p,
                    _ => return Err(Error::Runtime("files.lines requires a text path".to_string())),
                };
                let content = std::fs::read_to_string(path)
                    .map_err(|e| Error::Io(format!("Failed to read '{}': {}", path, e)))?;
                let lines: Vec<Value> = content
                    .lines()
                    .map(|l| Value::Text(l.to_string()))
                    .collect();
                Ok(Value::List(lines))
            },
            "files_delete" => {
                let path = match args.first() {
                    Some(Value::Text(p)) => p,
                    _ => return Err(Error::Runtime("files.delete requires a text path".to_string())),
                };
                std::fs::remove_file(path)
                    .map_err(|e| Error::Io(format!("Failed to delete '{}': {}", path, e)))?;
                Ok(Value::Nothing)
            },
            "files_copy" => {
                let (from, to) = match (&args.get(0), args.get(1)) {
                    (Some(Value::Text(f)), Some(Value::Text(t))) => (f, t),
                    _ => return Err(Error::Runtime("files.copy requires two text arguments".to_string())),
                };
                std::fs::copy(from, to)
                    .map(|_| Value::Nothing)
                    .map_err(|e| Error::Io(format!("Failed to copy '{}' to '{}': {}", from, to, e)))
            },
            "files_rename" => {
                let (from, to) = match (&args.get(0), args.get(1)) {
                    (Some(Value::Text(f)), Some(Value::Text(t))) => (f, t),
                    _ => return Err(Error::Runtime("files.rename requires two text arguments".to_string())),
                };
                std::fs::rename(from, to)
                    .map(|_| Value::Nothing)
                    .map_err(|e| Error::Io(format!("Failed to rename '{}' to '{}': {}", from, to, e)))
            },
            // Time module
            "time_now" => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| Error::Runtime(e.to_string()))?;
                let secs = now.as_secs();
                let nanos = now.subsec_nanos();
                let record = std::collections::HashMap::from([
                    ("seconds".to_string(), Value::Number(secs as f64)),
                    ("nanoseconds".to_string(), Value::Number(nanos as f64)),
                ]);
                Ok(Value::Record(record))
            },
            "time_sleep" => {
                let seconds = match args.first() {
                    Some(Value::Number(n)) => *n,
                    _ => return Err(Error::Runtime("time.sleep requires a number".to_string())),
                };
                std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
                Ok(Value::Nothing)
            },
            "time_format" => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let (timestamp, format) = match (&args.get(0), args.get(1)) {
                    (Some(Value::Number(ts)), Some(Value::Text(fmt))) => (*ts, fmt.clone()),
                    (Some(Value::Number(ts)), None) => (*ts, "%Y-%m-%d %H:%M:%S".to_string()),
                    _ => return Err(Error::Runtime("time.format requires a number and optional text".to_string())),
                };
                let datetime = UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64);
                let tm = chrono::DateTime::from_timestamp(datetime.as_secs() as i64, 0)
                    .ok_or_else(|| Error::Runtime("Invalid timestamp".to_string()))?;
                Ok(Value::Text(tm.format(&format).to_string()))
            },
            "time_unix" => {
                let text = match args.first() {
                    Some(Value::Text(s)) => s,
                    _ => return Err(Error::Runtime("time.unix requires a text".to_string())),
                };
                let parsed = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
                    .map_err(|_| Error::Runtime("Invalid date format, use YYYY-MM-DD HH:MM:SS".to_string()))?;
                Ok(Value::Number(parsed.and_utc().timestamp() as f64))
            },
            // Formats module (JSON/CSV)
            "json_parse" => {
                let text = match args.first() {
                    Some(Value::Text(s)) => s,
                    _ => return Err(Error::Runtime("json.parse requires text".to_string())),
                };
                parse_json(text).map_err(|e| Error::Runtime(e))
            },
            "json_stringify" => {
                let value = match args.first() {
                    Some(v) => v.clone(),
                    _ => return Err(Error::Runtime("json.stringify requires a value".to_string())),
                };
                Ok(Value::Text(json_stringify(&value)))
            },
            "csv_parse" => {
                let text = match args.first() {
                    Some(Value::Text(s)) => s,
                    _ => return Err(Error::Runtime("csv.parse requires text".to_string())),
                };
                let lines: Vec<Value> = text
                    .lines()
                    .map(|line| {
                        let cells: Vec<Value> = line
                            .split(',')
                            .map(|cell| Value::Text(cell.trim().to_string()))
                            .collect();
                        Value::List(cells)
                    })
                    .collect();
                Ok(Value::List(lines))
            },
            // Network module
            "network_get" => {
                let url = match args.first() {
                    Some(Value::Text(u)) => u,
                    _ => return Err(Error::Runtime("network.get requires a URL".to_string())),
                };
                let client = reqwest::blocking::Client::new();
                let response = client.get(url)
                    .send()
                    .map_err(|e| Error::Runtime(format!("HTTP request failed: {}", e)))?;
                let body = response.text()
                    .map_err(|e| Error::Runtime(format!("Failed to read response: {}", e)))?;
                Ok(Value::Text(body))
            },
            "network_post" => {
                let (url, data) = match (&args.get(0), args.get(1)) {
                    (Some(Value::Text(u)), Some(Value::Text(d))) => (u, d),
                    _ => return Err(Error::Runtime("network.post requires URL and data".to_string())),
                };
                let client = reqwest::blocking::Client::new();
                let response = client.post(url)
                    .body(data.clone())
                    .send()
                    .map_err(|e| Error::Runtime(format!("HTTP request failed: {}", e)))?;
                let body = response.text()
                    .map_err(|e| Error::Runtime(format!("Failed to read response: {}", e)))?;
                Ok(Value::Text(body))
            },
            // Testing module
            "expect" | "assert" => {
                let (actual, expected) = match (&args.get(0), args.get(1)) {
                    (Some(a), Some(e)) => (a.clone(), e.clone()),
                    _ => return Err(Error::Runtime("expect requires two arguments".to_string())),
                };
                if actual != expected {
                    return Err(Error::Runtime(format!(
                        "Assertion failed: expected {:?} but got {:?}",
                        expected, actual
                    )));
                }
                Ok(Value::Nothing)
            },
            // Console module
            "console_log" | "say" => {
                if let Some(arg) = args.first() {
                    println!("{}", arg);
                }
                Ok(Value::Nothing)
            },
            "console_error" => {
                if let Some(arg) = args.first() {
                    eprintln!("{}", arg);
                }
                Ok(Value::Nothing)
            },
            "console_clear" => {
                print!("\x1B[2J\x1B[1H");
                Ok(Value::Nothing)
            },
            // Random module
            "random_number" => {
                let (min, max) = match (&args.get(0), args.get(1)) {
                    (Some(Value::Number(min)), Some(Value::Number(max))) => (*min, *max),
                    (Some(Value::Number(max)), None) => (0.0, *max),
                    _ => (0.0, 1.0),
                };
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                let r = (now.as_nanos() % 1000000) as f64 / 1000000.0;
                Ok(Value::Number(min + r * (max - min)))
            },
            "random_choice" => {
                if let Some(Value::List(items)) = args.first().clone() {
                    if items.is_empty() {
                        return Ok(Value::Nothing);
                    }
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                    let idx = (now.as_nanos() as usize) % items.len();
                    Ok(items[idx].clone())
                } else {
                    Err(Error::Runtime("random_choice requires a list".to_string()))
                }
            },
            "random_shuffle" => {
                if let Some(Value::List(mut items)) = args.first().cloned() {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                    let seed = now.as_nanos() as usize;
                    
                    for i in (1..items.len()).rev() {
                        let j = (seed % (i + 1)) as usize;
                        items.swap(i, j);
                    }
                    Ok(Value::List(items))
                } else {
                    Err(Error::Runtime("random_shuffle requires a list".to_string()))
                }
            },
            // Type conversion
            "type_of" => {
                let type_name = match args.first() {
                    Some(Value::Number(_)) => "number",
                    Some(Value::Text(_)) => "text",
                    Some(Value::YesNo(_)) => "yes/no",
                    Some(Value::Nothing) => "nothing",
                    Some(Value::List(_)) => "list",
                    Some(Value::Record(_)) => "record",
                    Some(Value::Function(_, _)) => "function",
                    Some(Value::Builtin(_)) => "builtin",
                    Some(Value::Object(_, _)) => "object",
                    None => "nothing",
                };
                Ok(Value::Text(type_name.to_string()))
            },
            _ => {
                // User-defined function (simplified)
                if let Some(Value::Function(_, _)) = self.get_var(name) {
                    Ok(Value::Nothing)
                } else {
                    Err(Error::Runtime(format!("Unknown function '{}'", name)))
                }
            },
        }
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_json(json: &str) -> Result<Value> {
    let json = json.trim();
    if json.starts_with('{') {
        parse_json_object(json)
    } else if json.starts_with('[') {
        parse_json_array(json)
    } else if json.starts_with('"') {
        Ok(Value::Text(parse_json_string(json)?))
    } else if json == "null" {
        Ok(Value::Nothing)
    } else if json == "true" {
        Ok(Value::YesNo(true))
    } else if json == "false" {
        Ok(Value::YesNo(false))
    } else {
        match json.parse::<f64>() {
            Ok(n) => Ok(Value::Number(n)),
            Err(_) => Err(Error::Runtime(format!("Invalid JSON: {}", json))),
        }
    }
}

fn parse_json_object(json: &str) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('{') || !json.ends_with('}') {
        return Err(Error::Runtime("Invalid JSON object".to_string()));
    }
    let mut map = std::collections::HashMap::new();
    let content = &json[1..json.len()-1];
    if content.trim().is_empty() {
        return Ok(Value::Record(map));
    }
    for pair in split_json_pairs(content) {
        let parts: Vec<&str> = pair.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let key = parse_json_string(parts[0].trim())?;
        let value = parse_json(parts[1].trim())?;
        map.insert(key, value);
    }
    Ok(Value::Record(map))
}

fn parse_json_array(json: &str) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('[') || !json.ends_with(']') {
        return Err(Error::Runtime("Invalid JSON array".to_string()));
    }
    let content = &json[1..json.len()-1];
    if content.trim().is_empty() {
        return Ok(Value::List(Vec::new()));
    }
    let mut items = Vec::new();
    for item in split_json_elements(content) {
        items.push(parse_json(item)?);
    }
    Ok(Value::List(items))
}

fn parse_json_string(json: &str) -> Result<String> {
    let json = json.trim();
    if !json.starts_with('"') || !json.ends_with('"') {
        return Err(Error::Runtime("Invalid JSON string".to_string()));
    }
    let mut result = String::new();
    let chars: Vec<char> = json[1..json.len()-1].chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\"' => result.push('"'),
                '\\' => result.push('\\'),
                _ => result.push(chars[i + 1]),
            }
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    Ok(result)
}

fn split_json_pairs(content: &str) -> Vec<&str> {
    let mut pairs = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut in_string = false;
    for (i, c) in content.chars().enumerate() {
        if c == '"' && (i == 0 || content.chars().nth(i - 1) != Some('\\')) {
            in_string = !in_string;
        }
        if !in_string {
            if c == '{' || c == '[' {
                depth += 1;
            } else if c == '}' || c == ']' {
                depth -= 1;
            } else if c == ',' && depth == 0 {
                pairs.push(&content[start..i]);
                start = i + 1;
            }
        }
    }
    if start < content.len() {
        pairs.push(&content[start..]);
    }
    pairs
}

fn split_json_elements(content: &str) -> Vec<&str> {
    split_json_pairs(content)
}

fn json_stringify(value: &Value) -> String {
    match value {
        Value::Nothing => "null".to_string(),
        Value::YesNo(b) => if *b { "true".to_string() } else { "false".to_string() },
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        },
        Value::Text(s) => {
            let mut result = String::from("\"");
            for c in s.chars() {
                match c {
                    '\n' => result.push_str("\\n"),
                    '\r' => result.push_str("\\r"),
                    '\t' => result.push_str("\\t"),
                    '"' => result.push_str("\\\""),
                    '\\' => result.push_str("\\\\"),
                    _ => result.push(c),
                }
            }
            result.push('"');
            result
        },
        Value::List(items) => {
            let elements: Vec<String> = items.iter().map(json_stringify).collect();
            format!("[{}]", elements.join(", "))
        },
        Value::Record(fields) => {
            let pairs: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, json_stringify(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        },
        Value::Function(_, _) => "null".to_string(),
        Value::Builtin(_) => "null".to_string(),
        Value::Object(_, _) => "null".to_string(),
    }
}
