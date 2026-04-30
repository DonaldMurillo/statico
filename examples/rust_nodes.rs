use tree_sitter::{Node, Parser};

fn main() {
    let mut parser = Parser::new();
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&lang).unwrap();

    let code = r#"
pub fn hello() -> String { "hi".to_string() }
fn private_helper() {}

pub struct User { name: String, }
struct InternalStruct;

pub enum Status { Active, Inactive }
enum Internal {}

pub trait Draw { fn render(&self); }
trait InternalTrait {}

pub mod submod;
mod internal_mod;

pub const MAX: usize = 100;
const PRIVATE: i32 = 42;

pub type Result<T> = std::result::Result<T, Error>;

use std::collections::HashMap;
use crate::parser::AstParser;
use super::sibling_mod::SomeType;
use self::nested::NestedType;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
"#;

    let tree = parser.parse(code, None).unwrap();
    print_nodes(tree.root_node(), code, 0);
}

fn print_nodes(node: Node, code: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = if node.child_count() == 0 { &code[node.byte_range()] } else { "" };
    if !text.is_empty() {
        println!("{}{}: {:?}", indent, node.kind(), text);
    } else if depth < 4 {
        println!("{}{}", indent, node.kind());
    }
    for i in 0..node.child_count() {
        print_nodes(node.child(i).unwrap(), code, depth + 1);
    }
}
