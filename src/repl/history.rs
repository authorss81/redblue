use std::collections::VecDeque;

pub struct ReplHistory {
    history: VecDeque<String>,
    max_size: usize,
}

impl ReplHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push(&mut self, entry: String) {
        if self.history.back() == Some(&entry) {
            return;
        }

        if self.history.len() >= self.max_size {
            self.history.pop_front();
        }

        self.history.push_back(entry);
    }

    pub fn get(&self, index: usize) -> Option<&String> {
        self.history.get(index)
    }

    pub fn search(&self, prefix: &str) -> Option<String> {
        for entry in self.history.iter().rev() {
            if entry.starts_with(prefix) {
                return Some(entry.clone());
            }
        }
        None
    }

    pub fn search_all(&self, prefix: &str) -> Vec<String> {
        self.history
            .iter()
            .rev()
            .filter(|entry| entry.starts_with(prefix))
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    pub fn clear(&mut self) {
        self.history.clear();
    }

    pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;

        let mut file = File::create(path)?;
        for entry in &self.history {
            writeln!(file, "{}", entry)?;
        }
        Ok(())
    }

    pub fn load_from_file(&mut self, path: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open(path)?;
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                self.push(line);
            }
        }

        Ok(())
    }
}

impl Default for ReplHistory {
    fn default() -> Self {
        Self::new(1000)
    }
}
