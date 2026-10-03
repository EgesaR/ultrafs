use rustyline::{
    Context, Helper,
    completion::{Completer, Pair},
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};

pub struct CommandCompleter {
    commands: Vec<String>,
}

impl CommandCompleter {
    pub fn new() -> Self {
        Self {
            commands: vec![
                "help".into(),
                "spawn".into(),
                "focus".into(),
                "link".into(),
                "write".into(),
                "monitor".into(),
                "list".into(),
                "graph".into(),
                "delete".into(),
                "exit".into(),
                "ingest".into(),
                "stats".into(),
                "diskinfo".into(),
                "info".into(),
                "clear".into(),
            ],
        }
    }
}

// 1. Implement Completer for Tab Prediction
impl Completer for CommandCompleter {
    type Candidate = Pair;

    fn complete(
        &self, // FIXME should be `&mut self`
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Extract the word being typed
        let word = &line[..pos];

        // Only trigger autocompletion for the first word (the command itself)
        if line.contains(' ') {
            return Ok((0, Vec::new())); // No completion for arguments yet
        }

        let mut matches = Vec::new();
        for cmd in &self.commands {
            if cmd.starts_with(word) {
                matches.push(Pair {
                    display: cmd.clone(),
                    replacement: cmd.clone(),
                });
            }
        }

        Ok((0, matches))
    }
}

// 2. Dummy implementations required by Rustyline's Helper trait
impl Hinter for CommandCompleter {
    type Hint = String;
}
impl Highlighter for CommandCompleter {}
impl Validator for CommandCompleter {}
impl Helper for CommandCompleter {}
