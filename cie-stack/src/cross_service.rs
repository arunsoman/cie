//! Static Feign/service join. Route matches are declarations, never runtime calls.
use crate::types::*;
pub fn resolve_cross_service(report:&mut StackReport) {
    report.cross_service.clear();
    let outbound=report.jvm.as_ref().map(|j|j.outbound.clone()).unwrap_or_default();
    for ob in outbound.into_iter().filter(|o|o.kind=="feign") {
        let Some(target)=ob.target.as_deref().filter(|t|!t.contains("${")&&!t.contains("#{")) else {
            report.notes.push(format!("{}:{} outbound target is unresolved as written",ob.rel,ob.line));continue;
        };
        let Some(from)=report.apps.iter().filter(|a|a.dir.is_empty()||ob.rel.starts_with(&format!("{}/",a.dir))).max_by_key(|a|a.dir.len()) else {continue};
        let from=from.clone();
        let mut base="";
        let destination=if ob.target_kind=="url" {
            let Some(rest)=target.strip_prefix("http://").or_else(||target.strip_prefix("https://")) else {report.notes.push(format!("{}:{} unsupported outbound URL as written",ob.rel,ob.line));continue};
            let (authority,path)=rest.split_once('/').unwrap_or((rest,""));
            if authority.contains('@')||authority.contains('?')||path.contains('?')||path.contains('#') {report.notes.push(format!("{}:{} outbound URL cannot be statically joined",ob.rel,ob.line));continue;}
            base=path;
            authority.split(':').next().unwrap_or("")
        } else {target};
        let apps:Vec<_>=report.apps.iter().filter(|a|a.dir!=from.dir && (a.name.eq_ignore_ascii_case(destination)||a.dir.eq_ignore_ascii_case(destination))).collect();
        if apps.len()!=1 { report.notes.push(format!("{}:{} names outbound target {:?} — no unique CIE-parsed service matches it (external, unparsed or manifest/service-name mismatch)",ob.rel,ob.line,target)); continue; }
        let to=apps[0];
        let routes:Vec<_>=report.routes.iter().filter(|r|r.service==to.dir||r.service==to.name).filter(|r| {
            ob.requests.is_empty()||ob.requests.iter().any(|q| {
                let path=format!("/{}/{}",base.trim_matches('/'),q.path.trim_matches('/')).replace("//","/");
                r.path==path && r.verb.eq_ignore_ascii_case(&q.verb)
            })
        }).collect();
        // A service name alone cannot select an arbitrary endpoint.
        if routes.len()!=1 {report.notes.push(format!("{}:{} target {:?} found, but endpoint is unresolved or ambiguous ({} matching route declarations)",ob.rel,ob.line,target,routes.len()));continue;}
        let route=routes[0];
        report.cross_service.push(CrossServiceCall{from_app:from.name.clone(),to_app:to.name.clone(),via:ob.kind,matched_path:route.path.clone(),from_evidence:format!("{}:{}",ob.rel,ob.line),to_evidence:format!("{}:{}",route.rel,route.line),match_basis:if ob.requests.is_empty(){"unique service route; endpoint inferred"}else{"literal service identity + declared HTTP verb/path"}.into()});
    }
    report.cross_service.sort_by(|a,b|(&a.from_evidence,&a.to_evidence).cmp(&(&b.from_evidence,&b.to_evidence)));
}
