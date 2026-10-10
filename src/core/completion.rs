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
                "ingest".into(),
                "read".into(),
                "cat".into(),
                "rm".into(),
                "delete".into(),
                "checkpoint".into(),
                "snap".into(),
                "rollback".into(),
                "undo".into(),
                "monitor".into(),
                "diskinfo".into(),
                "info".into(),
                "stats".into(),
                "list".into(),
                "graph".into(),
                "clear".into(),
                "exit".into(),
            ],
        }
    }
}

impl Completer for CommandCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let word = &line[..pos];

        if line.contains(' ') {
            return Ok((0, Vec::new()));
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

impl Hinter for CommandCompleter {
    type Hint = String;
}
impl Highlighter for CommandCompleter {}
impl Validator for CommandCompleter {}
impl Helper for CommandCompleter {}
