pub struct ReplCompleter {
    keywords: Vec<String>,
    builtins: Vec<String>,
    commands: Vec<String>,
}

impl ReplCompleter {
    pub fn new() -> Self {
        Self {
            keywords: vec![
                "set".to_string(),
                "to".to_string(),
                "is".to_string(),
                "are".to_string(),
                "if".to_string(),
                "then".to_string(),
                "else".to_string(),
                "end".to_string(),
                "when".to_string(),
                "unless".to_string(),
                "for".to_string(),
                "each".to_string(),
                "in".to_string(),
                "from".to_string(),
                "to".to_string(),
                "by".to_string(),
                "times".to_string(),
                "while".to_string(),
                "repeat".to_string(),
                "until".to_string(),
                "break".to_string(),
                "skip".to_string(),
                "return".to_string(),
                "give back".to_string(),
                "say".to_string(),
                "print".to_string(),
                "ask".to_string(),
                "try".to_string(),
                "catch".to_string(),
                "finally".to_string(),
                "and".to_string(),
                "or".to_string(),
                "not".to_string(),
                "yes".to_string(),
                "no".to_string(),
                "nothing".to_string(),
                "module".to_string(),
                "import".to_string(),
                "export".to_string(),
                "object".to_string(),
                "has".to_string(),
                "can".to_string(),
                "this".to_string(),
                "new".to_string(),
                "extends".to_string(),
                "async".to_string(),
                "wait".to_string(),
                "parallel".to_string(),
                "done".to_string(),
            ],
            builtins: vec![
                "math".to_string(),
                "text".to_string(),
                "files".to_string(),
                "network".to_string(),
                "formats".to_string(),
                "list".to_string(),
                "console".to_string(),
                "PI".to_string(),
                "E".to_string(),
                "abs".to_string(),
                "floor".to_string(),
                "ceil".to_string(),
                "round".to_string(),
                "sqrt".to_string(),
                "pow".to_string(),
                "sin".to_string(),
                "cos".to_string(),
                "tan".to_string(),
                "length".to_string(),
                "push".to_string(),
                "pop".to_string(),
                "map".to_string(),
                "filter".to_string(),
                "reduce".to_string(),
                "random".to_string(),
                "uppercase".to_string(),
                "lowercase".to_string(),
                "trim".to_string(),
                "split".to_string(),
                "join".to_string(),
                "contains".to_string(),
                "read".to_string(),
                "write".to_string(),
                "exists".to_string(),
                "parse_json".to_string(),
                "to_json".to_string(),
            ],
            commands: vec![
                ":quit".to_string(),
                ":exit".to_string(),
                ":help".to_string(),
                ":clear".to_string(),
                ":history".to_string(),
                ":vars".to_string(),
                ":functions".to_string(),
                ":load".to_string(),
                ":save".to_string(),
                ":reset".to_string(),
                ":run".to_string(),
                ":debug".to_string(),
                ":inspect".to_string(),
                ":ast".to_string(),
                ":tokens".to_string(),
                ":example".to_string(),
            ],
        }
    }
    
    pub fn complete(&self, word: &str) -> Vec<String> {
        let mut matches = Vec::new();
        let word_lower = word.to_lowercase();
        
        for keyword in &self.keywords {
            if keyword.to_lowercase().starts_with(&word_lower) {
                matches.push(keyword.clone());
            }
        }
        
        for builtin in &self.builtins {
            if builtin.to_lowercase().starts_with(&word_lower) {
                matches.push(builtin.clone());
            }
        }
        
        for command in &self.commands {
            if command.to_lowercase().starts_with(&word_lower) {
                matches.push(command.clone());
            }
        }
        
        matches.sort();
        matches.dedup();
        
        matches
    }
}

impl Default for ReplCompleter {
    fn default() -> Self {
        Self::new()
    }
}
