use crossterm::event::KeyCode;

/// A single text input field with cursor navigation.
#[derive(Debug, Clone, Default)]
pub struct FieldInput {
    pub value: String,
    pub cursor: usize,  // byte offset
    pub masked: bool,
}

impl FieldInput {
    pub fn new(initial: &str) -> Self {
        let len = initial.len();
        Self { value: initial.to_string(), cursor: len, masked: false }
    }

    pub fn masked() -> Self {
        Self { masked: true, ..Self::default() }
    }

    /// Returns (before_display, after_display) split at cursor.
    /// When masked, each char is replaced by '•'.
    pub fn split_display_at_cursor(&self) -> (String, String) {
        let before = &self.value[..self.cursor];
        let after = &self.value[self.cursor..];
        if self.masked {
            (
                "•".repeat(before.chars().count()),
                "•".repeat(after.chars().count()),
            )
        } else {
            (before.to_string(), after.to_string())
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char(c) => {
                self.value.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    // Find the char boundary before cursor
                    let prev = self.prev_boundary();
                    self.value.drain(prev..self.cursor);
                    self.cursor = prev;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.value.len() {
                    let next = self.next_boundary();
                    self.value.drain(self.cursor..next);
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor = self.prev_boundary();
                }
            }
            KeyCode::Right => {
                if self.cursor < self.value.len() {
                    self.cursor = self.next_boundary();
                }
            }
            KeyCode::Home | KeyCode::Up => {
                self.cursor = 0;
            }
            KeyCode::End | KeyCode::Down => {
                self.cursor = self.value.len();
            }
            _ => {}
        }
    }

    fn prev_boundary(&self) -> usize {
        let mut pos = self.cursor;
        loop {
            if pos == 0 { return 0; }
            pos -= 1;
            if self.value.is_char_boundary(pos) { return pos; }
        }
    }

    fn next_boundary(&self) -> usize {
        let mut pos = self.cursor + 1;
        while pos <= self.value.len() {
            if self.value.is_char_boundary(pos) { return pos; }
            pos += 1;
        }
        self.value.len()
    }

}

/// A cycling select field (← / → or Space to cycle through options).
#[derive(Debug, Clone)]
pub struct SelectField {
    pub options: Vec<String>,
    pub idx: usize,
}

impl SelectField {
    pub fn new(options: Vec<&str>) -> Self {
        Self {
            options: options.iter().map(|s| s.to_string()).collect(),
            idx: 0,
        }
    }

    pub fn current(&self) -> &str {
        self.options.get(self.idx).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn next(&mut self) {
        if !self.options.is_empty() {
            self.idx = (self.idx + 1) % self.options.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.options.is_empty() {
            self.idx = self.idx.checked_sub(1).unwrap_or(self.options.len() - 1);
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Right | KeyCode::Char(' ') => self.next(),
            KeyCode::Left => self.prev(),
            _ => {}
        }
    }
}
