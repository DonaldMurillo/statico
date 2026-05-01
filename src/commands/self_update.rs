//! `statico update` command.

use std::process;

pub fn run_update(check_only: bool) {
    match statico::update::run_update(check_only) {
        Ok(msg) => println!("{}", msg),
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}
