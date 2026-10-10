use colored::*;

pub enum Status {
    Success,
    Error,
    Warning,
    Info,
    System,
    ActiveItem,
    File,
}

impl Status {
    pub fn icon(&self) -> &str {
        match self {
            Status::Success => "✓",
            Status::Error => "✗",
            Status::Warning => "!",
            Status::Info => "i",
            Status::System => "◆",
            Status::ActiveItem => "›",
            Status::File => "·",
        }
    }
}

pub fn print_status(status: Status, message: &str) {
    let prefix = match status {
        Status::Success => status.icon().green(),
        Status::Error => status.icon().red(),
        Status::Warning => status.icon().yellow(),
        Status::Info => status.icon().cyan(),
        Status::System => status.icon().magenta(),
        _ => status.icon().normal(),
    };

    println!("{} {}", prefix, message);
}

pub fn print_tree(is_last: bool, message: &str) {
    let branch = if is_last { "└─" } else { "├─" };
    println!("{} {}", branch.dimmed(), message)
}
