use tree_sitter::Parser;

fn main() {
    let source = r#"import { check } from './conditional';
import('./lazy');
import('./feature').then(m => m.run());
console.log(check);
"#;
    
    let mut parser = Parser::new();
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    
    // Print full tree
    print_tree(root, source, 0);
}

fn print_tree(node: tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let text_preview: String = text.chars().take(40).collect();
    println!("{}{} [{}, {}..{}] {:?}", 
        indent, 
        node.kind(),
        node.start_position().row,
        node.start_position().column,
        node.end_position().column,
        text_preview
    );
    for i in 0..node.child_count() {
        print_tree(node.child(i).unwrap(), source, depth + 1);
    }
}
