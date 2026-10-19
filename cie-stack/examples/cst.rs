// debug: print node kinds of a file, indented, filtered by a substring
use std::fs;

fn walk(node: tree_sitter::Node, src: &str, depth: usize, keep: &str) {
    let kind = node.kind();
    let txt: String = node.utf8_text(src.as_bytes()).unwrap_or("").chars().take(46).collect::<String>().replace('\n', "\\n");
    if keep.is_empty() || kind.contains(keep) || depth == 0 {
        println!("{}[{}] {}", "  ".repeat(depth), kind, txt);
    }
    if depth < 16 {
        let mut cursor = node.walk();
        for c in node.children(&mut cursor) {
            walk(c, src, depth + 1, keep);
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("file path");
    let lang = std::env::args().nth(2).unwrap_or_default();
    let keep = std::env::args().nth(3).unwrap_or_default();
    let src = fs::read_to_string(&path).unwrap();
    let mut p = tree_sitter::Parser::new();
    match lang.as_str() {
        "java" => p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap(),
        "kt" => p.set_language(&tree_sitter_kotlin_ng::LANGUAGE.into()).unwrap(),
        l => panic!("lang {l}"),
    };
    let tree = p.parse(&src, None).unwrap();
    walk(tree.root_node(), &src, 0, &keep);
}
