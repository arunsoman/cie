// debug: what does spring::parse_file see in this file?
use cie_stack::spring;
use std::fs;

fn dump_stmt(s: &spring::Stmt, ind: usize) {
    let pad = "  ".repeat(ind);
    match s {
        spring::Stmt::If { cond, if_arm, else_arm, .. } => {
            println!("{pad}if {cond} (if={:?} else={:?})", if_arm.len(), else_arm.as_ref().map(|e| e.len()));
            for x in if_arm { dump_stmt(x, ind+1); }
            if let Some(e) = else_arm { println!("{pad}ELSE:"); for x in e { dump_stmt(x, ind+1); } }
        }
        spring::Stmt::When { label, cases, .. } => {
            println!("{pad}when {label} ({} cases)", cases.len());
            for (l, arm) in cases { println!("{pad}  case {l} ({} stmts)", arm.len()); for x in arm { dump_stmt(x, ind+2); } }
        }
        spring::Stmt::Throw { text, .. } => println!("{pad}throw {text}"),
        spring::Stmt::Return { text, .. } => println!("{pad}return {text}"),
        spring::Stmt::Call { recv, method, .. } => println!("{pad}call {:?}.{}", recv, method),
        spring::Stmt::Other { text, .. } => println!("{pad}other {text}"),
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let lang = std::env::args().nth(2).unwrap_or_default();
    let lang = if lang == "kt" { "kotlin" } else { "java" };
    let src = fs::read_to_string(&path).unwrap();
    let classes = spring::parse_file(if lang == "kotlin" { "kotlin" } else { "java" }, &path, &src);
    for c in classes {
        println!("CLASS {} ({}:{} line {})", c.name, c.lang, c.rel, c.line);
        println!("  anns: {:?}", c.anns.iter().map(|a| format!("@{}{}", a.name, a.args)).collect::<Vec<_>>());
        println!("  injections: {:?}", c.injections.iter().map(|i| format!("{}:{}", i.field, i.r#type)).collect::<Vec<_>>());
        println!("  props: {:?}", c.props.iter().map(|p| format!("{}:{}", p.name, p.r#type)).collect::<Vec<_>>());
        println!("  repo: {:?}/{:?}", c.repo_extends, c.repo_entity);
        for m in &c.methods {
            println!("  METHOD {} line {} params={:?} rets={:?}", m.name, m.line, m.params, m.returns);
            println!("    anns: {:?}", m.anns.iter().map(|a| format!("@{}{}", a.name, a.args)).collect::<Vec<_>>());
            for s in &m.body { dump_stmt(s, 3); }
        }
    }
}
