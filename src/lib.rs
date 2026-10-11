pub mod ai;
pub mod cache;
pub mod cli;
pub mod core;
pub mod fastcdc;
pub mod graph;
pub mod hardware;

// Re-export IoCommand at the crate root so `crate::IoCommand` resolves correctly across modules
pub use hardware::io_worker::IoCommand;