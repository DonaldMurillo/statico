use tree_sitter::Parser;

fn main() {
    let code = r#"
cfg_feature! {
    #![feature = "client"]
    pub mod client;
}

cfg_proto! {
    mod headers;
    mod proto;
}
"#;
    let mut parser = Parser::new();
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(code, None).unwrap();

    fn print_tree(node: tree_sitter::Node, code: &str, indent: usize) {
        let text = if node.child_count() == 0 { &code[node.byte_range()] } else { "" };
        println!("{:indent$}{} {:?}", "", node.kind(), text);
        for child in node.children(&mut node.walk()) {
            print_tree(child, code, indent + 2);
        }
    }
    print_tree(tree.root_node(), code, 0);
}
