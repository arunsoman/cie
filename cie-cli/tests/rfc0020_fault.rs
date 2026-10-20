use std::{fs,process::Command};
#[test]
fn fault_cli_exports_static_only_and_degrades_without_model() {
 let root=std::env::temp_dir().join(format!("cie-fault-cli-{}-{}",std::process::id(),cie_core::types::now_ns()));fs::create_dir_all(&root).unwrap();
 fs::write(root.join("app.py"),"def greet():\n    print('hello')\n").unwrap();
 let invoke=|args:&[&str]| {let o=Command::new(env!("CARGO_BIN_EXE_cie")).args(args).output().unwrap();assert!(o.status.success(),"{}",String::from_utf8_lossy(&o.stderr));serde_json::from_slice::<serde_json::Value>(&o.stdout).unwrap()};
 let path=root.to_str().unwrap();let empty=invoke(&["fault",path,"--json"]);assert_eq!(empty["status"],"Unknown");assert!(empty["unknown_reason"].as_str().unwrap().contains("NoEvidencedStateModel"));
 fs::write(root.join("app.py"),"def authorize():\n    print('hello')\n    return True\n").unwrap();
 let state=invoke(&["state-model",path,"--json"]);assert!(state["transitions"].as_array().unwrap().len()>0);
 let report=invoke(&["fault",path,"--json"]);assert_eq!(report["status"],"Simulated");assert_eq!(report["execution_performed"],false);assert_eq!(report["runtime_observed"],false);assert!(report["lineage"]["edges"].as_array().unwrap().iter().all(|e|e["id"].as_str().unwrap().starts_with("static:")));
 let zero=invoke(&["fault",path,"--json","--budget-ms","0"]);assert_eq!(zero["status"],"Unknown");assert_eq!(zero["unknown_reason"],"TimedOut");
 assert!(!Command::new(env!("CARGO_BIN_EXE_cie")).args(["fault",path,"--max-faults","0"]).output().unwrap().status.success());
 fs::remove_dir_all(root).unwrap();
}
