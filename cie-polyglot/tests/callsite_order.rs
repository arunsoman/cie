use cie_polyglot::{schema,ir};
#[test]
fn nested_calls_are_retained_in_evaluation_order_without_executing_closure_bodies(){
 let root=std::env::temp_dir().join(format!("cie-call-order-{}",std::process::id()));std::fs::create_dir_all(&root).unwrap();
 std::fs::write(root.join("Example.java"),"class Example { int run() { return outer(inner(), inner()); } void enqueue() { executor.execute(() -> deferred()); } int inner(){return 1;} int outer(int a,int b){return a+b;} void deferred(){} }").unwrap();
 let db=root.join("capture.db");let conn=schema::open_db(&db).unwrap();ir::ingest_and_lower_reusing(&conn,1,1,&root,None).unwrap();
 let mut query=conn.prepare("SELECT c.callee_text FROM calls c JOIN symbols s ON s.id=c.caller_symbol_id WHERE s.name='run' ORDER BY c.stmt_index").unwrap();
 let calls:Vec<String>=query.query_map([],|r|r.get(0)).unwrap().map(Result::unwrap).collect();
 assert_eq!(calls,vec!["inner","inner","outer"]);
 let body_calls:i64=conn.query_row("SELECT count(*) FROM calls c JOIN symbols s ON s.id=c.caller_symbol_id WHERE s.name='enqueue' AND c.callee_text='deferred'",[],|r|r.get(0)).unwrap();
 assert_eq!(body_calls,0,"creating a lambda is not executing its body");
 drop(query);drop(conn);std::fs::remove_dir_all(root).unwrap();
}
