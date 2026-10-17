fn main() {
    let src = r#"
import os
from db import cursor

def authorize(user):
    print(f"checking {user}")
    return True

def charge(amount, user):
    ok = authorize(user)
    cursor.execute(f"UPDATE accounts SET balance = balance - {amount} WHERE user = '{user}'")
    try:
        charge_card(amount)
    except:
        print("charge failed, ignoring")
    return 200

def good_auth(user):
    if verify_password(user.name, user.pw):
        return True
    return False

def verify_password(name, pw):
    return check_hash(name, pw)

def delete_user(user):
    os.system("rm -rf /home/" + user)
"#;
    let proj = cie_langs::extract_project(
        &vec![("bank.py".to_string(), cie_core::types::LangId::Python, src.to_string())],
        &cie_langs::default_registry(),
    );
    for f in &proj.per_file {
        println!("== symbols ==");
        for s in &f.symbols { println!("  {} {} {}", s.id, s.kind_str(), s.qual_name); }
        println!("== calls ==");
        for c in &f.calls { println!("  {} '{}' -> {:?} in_loop={}", c.line, c.callee_text, c.callee, c.in_loop); }
        println!("== returns ==");
        for r in &f.returns { println!("  {} '{:30}' const={:?}", r.line, r.expr, r.constant); }
        println!("== exceptions ==");
        for e in &f.exceptions { println!("  {} kind='{}' bare={}", e.line, e.kind_text, e.is_bare); }
        println!("== sinks ==");
        for s in &f.sinks { println!("  {} {:?} '{}' param={}", s.line, s.kind, s.call_text, s.parameterized); }
        println!("== sources ==");
        for s in &f.sources { println!("  {} var='{}' {:?}", s.line, s.var, s.kind); }
        println!("== assignments ==");
        for a in &f.assignments { println!("  {} {} = {:?} {:?}", a.line, a.var, a.rhs_kind, a.rhs_data); }
        println!("== loops ==");
        for l in &f.loops { println!("  {} {:?} body {}-{}", l.line, l.kind, l.body_start, l.body_end); }
        println!("unsupported: {:?}", f.unsupported);
    }
}
