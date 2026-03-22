use crate::error::{Error, Result};
use crate::value::Value;
use std::io::{self, Write};

pub fn call(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "say" => say(args),
        "print" => print(args),
        "println" => println(args),
        "ask" => ask(args),
        "ask_password" | "askPassword" => ask_password(args),
        "clear" => clear(args),
        "log" => log(args),
        "warn" => warn(args),
        "error" => error(args),
        "table" => table(args),
        "progress" => progress(args),
        "spinner" => spinner(args),
        "color" => color(args),
        "style" => style(args),
        "cursor" => cursor(args),
        "beep" => beep(args),
        "read_key" | "readKey" => read_key(args),
        "read_line" | "readLine" => read_line(args),
        _ => Err(Error::Runtime(format!("Unknown console function '{}'", function))),
    }
}

fn say(args: Vec<Value>) -> Result<Value> {
    for arg in args {
        print!("{}", arg);
    }
    println!();
    io::stdout().flush().ok();
    Ok(Value::Nothing)
}

fn print(args: Vec<Value>) -> Result<Value> {
    for arg in args {
        print!("{}", arg);
    }
    io::stdout().flush().ok();
    Ok(Value::Nothing)
}

fn println(args: Vec<Value>) -> Result<Value> {
    for arg in args {
        println!("{}", arg);
    }
    Ok(Value::Nothing)
}

fn ask(args: Vec<Value>) -> Result<Value> {
    let prompt = args.first().map(|v| v.to_string()).unwrap_or_default();
    
    print!("{}", prompt);
    io::stdout().flush().ok();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|e| Error::Runtime(e.to_string()))?;
    
    Ok(Value::Text(input.trim().to_string()))
}

fn ask_password(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("ask_password not implemented - requires termios".to_string()))
}

fn clear(_args: Vec<Value>) -> Result<Value> {
    print!("\x1B[2J\x1B[H");
    io::stdout().flush().ok();
    Ok(Value::Nothing)
}

fn log(args: Vec<Value>) -> Result<Value> {
    let prefix = "[INFO] ";
    for arg in args {
        eprint!("{}{}", prefix, arg);
    }
    eprintln!();
    Ok(Value::Nothing)
}

fn warn(args: Vec<Value>) -> Result<Value> {
    let prefix = "[WARN] ";
    for arg in args {
        eprint!("\x1B[33m{}{}\x1B[0m", prefix, arg);
    }
    eprintln!();
    Ok(Value::Nothing)
}

fn error(args: Vec<Value>) -> Result<Value> {
    let prefix = "[ERROR] ";
    for arg in args {
        eprint!("\x1B[31m{}{}\x1B[0m", prefix, arg);
    }
    eprintln!();
    Ok(Value::Nothing)
}

fn table(args: Vec<Value>) -> Result<Value> {
    let items = expect_list(args.first())?;
    
    if items.is_empty() {
        return Ok(Value::Nothing);
    }
    
    if let Value::Record(fields) = &items[0] {
        let headers: Vec<&String> = fields.keys().collect();
        let widths: Vec<usize> = headers.iter()
            .map(|h| {
                let col_width = fields.get(*h).map(|v| v.to_string().len()).unwrap_or(0);
                h.len().max(col_width)
            })
            .collect();
        
        let separator: String = widths.iter()
            .map(|w| format!("-{}-", "-".repeat(*w)))
            .collect::<Vec<_>>()
            .join("+");
        
        println!("+{}+", separator);
        
        for (i, h) in headers.iter().enumerate() {
            print!("| {:<width$} ", h, width = widths[i]);
        }
        println!("|");
        println!("+{}+", separator);
        
        for item in items {
            if let Value::Record(fields) = item {
                for (i, h) in headers.iter().enumerate() {
                    let value = fields.get(*h).map(|v| v.to_string()).unwrap_or_default();
                    print!("| {:<width$} ", value, width = widths[i]);
                }
                println!("|");
            }
        }
        
        println!("+{}+", separator);
    } else {
        for item in items {
            println!("{}", item);
        }
    }
    
    Ok(Value::Nothing)
}

fn progress(args: Vec<Value>) -> Result<Value> {
    let current = expect_number(args.first())?;
    let total = expect_number(args.get(1))?;
    let width = args.get(2).map(|v| expect_number(Some(v)).unwrap_or(40.0) as usize).unwrap_or(40);
    let label = args.get(3).map(|v| v.to_string()).unwrap_or_default();
    
    let percentage = if total > 0.0 { current / total } else { 0.0 };
    let filled = ((percentage * width as f64) as usize).min(width);
    let empty = width - filled;
    
    print!("\r[{}{}] {:3.0}% {}", 
        "=".repeat(filled), 
        " ".repeat(empty),
        percentage * 100.0,
        label
    );
    io::stdout().flush().ok();
    
    if current >= total {
        println!();
    }
    
    Ok(Value::Nothing)
}

fn spinner(_args: Vec<Value>) -> Result<Value> {
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    
    for (i, frame) in frames.iter().enumerate() {
        print!("\r{} Loading...", frame);
        io::stdout().flush().ok();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    
    print!("\r");
    io::stdout().flush().ok();
    
    Ok(Value::Nothing)
}

fn color(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let color_code = args.get(1).and_then(|v| {
        match v {
            Value::Text(s) => color_name_to_code(s),
            Value::Number(n) => Some(*n as u8),
            _ => None,
        }
    }).unwrap_or(37);
    
    println!("\x1B[{}m{}\x1B[0m", color_code, text);
    Ok(Value::Nothing)
}

fn color_name_to_code(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        "black" => Some(30),
        "red" => Some(31),
        "green" => Some(32),
        "yellow" => Some(33),
        "blue" => Some(34),
        "magenta" | "purple" => Some(35),
        "cyan" => Some(36),
        "white" => Some(37),
        "default" => Some(39),
        "bold" => Some(1),
        "dim" => Some(2),
        "italic" => Some(3),
        "underline" => Some(4),
        _ => None,
    }
}

fn style(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let styles: Vec<u8> = args.iter()
        .skip(1)
        .filter_map(|v| {
            if let Value::Text(s) = v {
                color_name_to_code(s)
            } else if let Value::Number(n) = v {
                Some(*n as u8)
            } else {
                None
            }
        })
        .collect();
    
    let codes = styles.join(";");
    println!("\x1B[{}m{}\x1B[0m", codes, text);
    Ok(Value::Nothing)
}

fn cursor(args: Vec<Value>) -> Result<Value> {
    let action = expect_text(args.first())?;
    
    match action.as_str() {
        "show" => {
            print!("\x1B[?25h");
        },
        "hide" => {
            print!("\x1B[?25l");
        },
        "up" => {
            let n = args.get(1).map(|v| expect_number(Some(v)).unwrap_or(1.0) as u32).unwrap_or(1);
            print!("\x1B[{}A", n);
        },
        "down" => {
            let n = args.get(1).map(|v| expect_number(Some(v)).unwrap_or(1.0) as u32).unwrap_or(1);
            print!("\x1B[{}B", n);
        },
        "forward" | "right" => {
            let n = args.get(1).map(|v| expect_number(Some(v)).unwrap_or(1.0) as u32).unwrap_or(1);
            print!("\x1B[{}C", n);
        },
        "back" | "left" => {
            let n = args.get(1).map(|v| expect_number(Some(v)).unwrap_or(1.0) as u32).unwrap_or(1);
            print!("\x1B[{}D", n);
        },
        "home" => {
            print!("\x1B[H");
        },
        "save" => {
            print!("\x1B[s");
        },
        "restore" => {
            print!("\x1B[u");
        },
        "clear_line" | "clearLine" => {
            print!("\x1B[2K\r");
        },
        _ => {
            return Err(Error::Runtime(format!("Unknown cursor action '{}'", action)));
        }
    }
    
    io::stdout().flush().ok();
    Ok(Value::Nothing)
}

fn beep(_args: Vec<Value>) -> Result<Value> {
    print!("\x07");
    io::stdout().flush().ok();
    Ok(Value::Nothing)
}

fn read_key(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("read_key not implemented - requires termios".to_string()))
}

fn read_line(args: Vec<Value>) -> Result<Value> {
    let prompt = args.first().map(|v| v.to_string()).unwrap_or_default();
    
    print!("{}", prompt);
    io::stdout().flush().ok();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|e| Error::Runtime(e.to_string()))?;
    
    Ok(Value::Text(input.trim_end_matches('\n').to_string()))
}

fn expect_text(value: Option<&Value>) -> Result<String> {
    match value {
        Some(Value::Text(s)) => Ok(s.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected text, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}

fn expect_number(value: Option<&Value>) -> Result<f64> {
    match value {
        Some(Value::Number(n)) => Ok(*n),
        Some(v) => Err(Error::Runtime(format!("Expected number, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}

fn expect_list(value: Option<&Value>) -> Result<Vec<Value>> {
    match value {
        Some(Value::List(items)) => Ok(items.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected list, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}
