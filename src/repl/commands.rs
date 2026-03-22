pub enum ReplCommand {
    Quit,
    Help,
    Clear,
    History,
    Vars,
    Functions,
    Load(String),
    Save(String),
    Reset,
    Run(String),
    Debug,
    Inspect(String),
    Ast(String),
    Tokens(String),
    Example,
    Unknown(String),
}

impl ReplCommand {
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        
        if !input.starts_with(':') && !input.starts_with('.') {
            return ReplCommand::Unknown(input.to_string());
        }
        
        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0].trim_start_matches(':').trim_start_matches('.');
        
        match cmd {
            "quit" | "exit" | "q" => ReplCommand::Quit,
            "help" | "h" | "?" => ReplCommand::Help,
            "clear" | "cls" => ReplCommand::Clear,
            "history" | "hist" => ReplCommand::History,
            "vars" | "variables" | "v" => ReplCommand::Vars,
            "functions" | "funcs" | "f" => ReplCommand::Functions,
            "load" | "l" => {
                if let Some(path) = parts.get(1) {
                    ReplCommand::Load(path.to_string())
                } else {
                    ReplCommand::Unknown("load requires a file path".to_string())
                }
            },
            "save" | "s" => {
                if let Some(path) = parts.get(1) {
                    ReplCommand::Save(path.to_string())
                } else {
                    ReplCommand::Unknown("save requires a file path".to_string())
                }
            },
            "reset" => ReplCommand::Reset,
            "run" => {
                if let Some(path) = parts.get(1) {
                    ReplCommand::Run(path.to_string())
                } else {
                    ReplCommand::Unknown("run requires a file path".to_string())
                }
            },
            "debug" => ReplCommand::Debug,
            "inspect" | "i" => {
                if let Some(name) = parts.get(1) {
                    ReplCommand::Inspect(name.to_string())
                } else {
                    ReplCommand::Unknown("inspect requires a variable name".to_string())
                }
            },
            "ast" => {
                if let Some(expr) = parts.get(1) {
                    ReplCommand::Ast(expr.to_string())
                } else {
                    ReplCommand::Unknown("ast requires an expression".to_string())
                }
            },
            "tokens" => {
                if let Some(expr) = parts.get(1) {
                    ReplCommand::Tokens(expr.to_string())
                } else {
                    ReplCommand::Unknown("tokens requires an expression".to_string())
                }
            },
            "example" | "examples" => ReplCommand::Example,
            _ => ReplCommand::Unknown(cmd.to_string()),
        }
    }
}
