//! Docker-awareness — read the project's own container files as written.

use crate::types::{ComposeFile, ComposeService, DockerInfo, Dockerfile};
use crate::CapturedInputs;

/// scan the tree for Dockerfile(s) + docker-compose files; the files' text
/// comes from the capture when the ingest walk already read them, disk otherwise
pub(crate) fn docker_info(inputs: &CapturedInputs, rels: &[String]) -> DockerInfo {
    let mut dockerfiles: Vec<Dockerfile> = Vec::new();
    let mut compose: Option<ComposeFile> = None;
    let mut read_names: Vec<String> = Vec::new();

    for rel in rels {
        let base = rel.rsplit('/').next().unwrap_or(rel);
        if is_dockerfile(base) {
            if let Some(src) = inputs.read_input(rel) {
                read_names.push(rel.clone());
                dockerfiles.push(parse_dockerfile(rel, &src));
            }
        } else if is_compose(base) {
            if let Some(src) = inputs.read_input(rel) {
                read_names.push(rel.clone());
                compose = Some(parse_compose(rel, &src));
            }
        }
    }

    let present = !dockerfiles.is_empty() || compose.is_some();
    let mut note = String::new();
    if present {
        let mut parts: Vec<String> = Vec::new();
        if let Some(c) = &compose {
            parts.push(format!(
                "docker compose with {} service(s): {}",
                c.services.len(),
                c.services.iter().take(4).map(|s| s.name.clone()).collect::<Vec<_>>().join(", ")
            ));
        }
        if !dockerfiles.is_empty() {
            let base = dockerfiles.iter().find_map(|d| d.base_image.clone());
            parts.push(format!(
                "{} Dockerfile(s){}",
                dockerfiles.len(),
                base.map(|b| format!(" (base image {b})")).unwrap_or_default()
            ));
        }
        parts.push("read as written; CIE doesn't build or run containers.".to_string());
        note = parts.join(" · ");
    }

    DockerInfo {
        present,
        dockerfiles,
        compose,
        note,
    }
}

pub fn is_dockerfile(name: &str) -> bool {
    name == "Dockerfile" || name.starts_with("Dockerfile.")
}

fn is_compose(name: &str) -> bool {
    name == "docker-compose.yml"
        || name == "docker-compose.yaml"
        || name == "compose.yml"
        || name == "compose.yaml"
}

fn parse_dockerfile(rel: &str, src: &str) -> Dockerfile {
    let mut base_image = None;
    let mut exposed_ports: Vec<u32> = Vec::new();
    for line in src.lines() {
        if line.starts_with("FROM ") && base_image.is_none() {
            base_image = line.split_whitespace().nth(1).map(|s| s.to_string());
        } else if line.starts_with("EXPOSE ") {
            for tok in line.split_whitespace().skip(1) {
                if let Ok(p) = tok.parse::<u32>() {
                    exposed_ports.push(p);
                }
            }
        }
    }
    Dockerfile {
        path: rel.to_string(),
        base_image,
        exposed_ports,
    }
}

/// compose yaml — small indent-based read (services: entries, their
/// `image:`/`build:` and both port forms). Not a yaml library: the files CIE
/// needs from compose are flat and named. RFC-0019 §6.3: `build:` is THE
/// field that ties a service to an app directory — scalar (`build: ./x`)
/// and block (`build:` + deeper `context:`/`dockerfile:`) forms both read;
/// inline flow port lists (`ports: ["8081:8081"]`) read beside list rows.
/// Every service carries its own key row so the governance join can cite
/// `file:line` as written. Rows that name nothing real (no image, no build)
/// and duplicate keys are skipped, honestly, by name.
fn parse_compose(rel: &str, src: &str) -> ComposeFile {
    let mut services: Vec<ComposeService> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut in_services = false;
    // the service the current rows belong to — None while under a skipped
    // (duplicate) key, so its rows are ignored, never attributed to the
    // previous service
    let mut cur: Option<usize> = None;
    // inside a block `build:` — only its context:/dockerfile: rows fill it;
    // a `context:` under environment:/labels: names nothing about builds
    let mut in_build = false;
    for (idx, raw) in src.lines().enumerate() {
        let row = (idx + 1) as u32;
        let line_no_comment = raw.split(" #").next().unwrap_or(raw);
        if line_no_comment.trim_start().starts_with('#') || line_no_comment.trim().is_empty() {
            continue;
        }
        let indent = line_no_comment.len() - line_no_comment.trim_start().len();
        if indent == 0 {
            in_services = line_no_comment.trim() == "services:";
            continue;
        }
        if !in_services {
            continue;
        }
        let text = line_no_comment.trim();
        if indent == 2 {
            // a service key — "name:" with nothing (or a value) after
            in_build = false;
            let name = text.split(':').next().unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            if services.iter().any(|s| s.name == name) {
                skipped.push(name);
                cur = None; // duplicate key — keep the first; its rows are ignored
                continue;
            }
            services.push(ComposeService {
                name,
                image: None,
                build: None,
                dockerfile: None,
                ports: Vec::new(),
                line: row,
            });
            cur = Some(services.len() - 1);
            continue;
        }
        if indent >= 4 {
            let Some(idx) = cur else { continue };
            if indent == 4 {
                in_build = false; // any 4-indent row ends the build block
            }
            let svc = &mut services[idx];
            if let Some(rest) = text.strip_prefix("image:") {
                let v = rest.trim();
                if !v.is_empty() {
                    svc.image = Some(v.trim_matches(|c| c == '"' || c == '\'').to_string());
                }
            } else if let Some(rest) = text.strip_prefix("ports:") {
                // inline flow list: ports: ["8081:8081", 8082:8082]
                let v = rest.trim();
                if v.starts_with('[') && v.ends_with(']') {
                    for item in v[1..v.len() - 1].split(',') {
                        let p = item.trim().trim_matches(|c| c == '"' || c == '\'');
                        if !p.is_empty() {
                            svc.ports.push(p.to_string());
                        }
                    }
                }
            } else if let Some(rest) = text.strip_prefix("build:") {
                // scalar form here; an empty value means the block form —
                // its `context:` row (deeper) fills it below.
                let v = rest.trim();
                svc.build = Some(if v.is_empty() {
                    String::new()
                } else {
                    v.trim_matches(|c| c == '"' || c == '\'').to_string()
                });
                in_build = v.is_empty();
            } else if let Some(rest) = text.strip_prefix("- ") {
                let p = rest.trim().trim_matches(|c| c == '"' || c == '\'');
                if p.contains(':') && p.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    svc.ports.push(p.to_string());
                }
            } else if in_build && indent > 4 {
                // deeper rows under a block `build:` — and ONLY those
                if let Some(rest) = text.strip_prefix("context:") {
                    let v = rest.trim().trim_matches(|c| c == '"' || c == '\'');
                    if !v.is_empty() {
                        svc.build = Some(v.to_string());
                    }
                } else if let Some(rest) = text.strip_prefix("dockerfile:") {
                    let v = rest.trim().trim_matches(|c| c == '"' || c == '\'');
                    if !v.is_empty() {
                        svc.dockerfile = Some(v.to_string());
                    }
                }
            }
        }
    }
    // a block `build:` whose context row never appeared names nothing —
    // a service with neither image nor build is skipped, honestly, by name.
    services.retain(|s| {
        let named = s.image.is_some() || !s.build.as_deref().unwrap_or("").is_empty();
        if !named {
            skipped.push(s.name.clone());
        }
        named
    });
    ComposeFile {
        path: rel.to_string(),
        services,
        skipped,
    }
}