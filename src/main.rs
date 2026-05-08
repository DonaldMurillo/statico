//! statico CLI — Static code analyzer for TypeScript and Rust projects.

mod commands;

fn main() {
    commands::cli::parse_and_dispatch();
}
