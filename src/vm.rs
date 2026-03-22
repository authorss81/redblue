use crate::error::{Error, Result};
use crate::parser::{Program, Statement, Expr, BinaryOp, UnaryOp};
use crate::value::Value;
use crate::stdlib;
use std::collections::HashMap;

pub struct Vm {
    globals: HashMap<String, Value>,
    locals: Vec<HashMap<String, Value>>,
    functions: HashMap<String, Vec<String>>,
    output: Vec<String>,
}

impl Vm {
    pub fn new() -> Self {
        let mut globals = stdlib::builtins();
        Self {
            globals,
            locals: vec![HashMap::new()],
            functions: HashMap::new(),
            output: Vec::new(),
        }
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
            Statement::Import(module) => {
                // Module loading - simplified
                println!("Importing module: {}", module);
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
