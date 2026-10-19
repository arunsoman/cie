//! Docker-awareness — read the project's own container files as written.

use crate::types::{ComposeFile, ComposeService, DockerInfo, Dockerfile};
use std::fs;
use std::path::Path;

/// scan the tree for Dockerfile(s) + docker-compose files
pub fn docker_info(root: &Path, rels: &[String]) -> DockerInfo {
    let mut dockerfiles: Vec<Dockerfile> = Vec::new();
    let mut compose: Option<ComposeFile> = None;
    let mut read_names: Vec<String> = Vec::new();

    for rel in rels {
        let base = rel.rsplit('/').next().unwrap_or(rel);
        let path = root.join(rel);
        if is_dockerfile(base) {
            if let Ok(src) = fs::read_to_string(&path) {
                read_names.push(rel.clone());
                dockerfiles.push(parse_dockerfile(rel, &src));
            }
        } else if is_compose(base) {
            if let Ok(src) = fs::read_to_string(&path) {
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

/// compose yaml — small indent-based read (only `services:` entries,
/// their `image:` and list-port rows). Not a yaml library: the files CIE
/// needs from compose are flat and named.
fn parse_compose(rel: &str, src: &str) -> ComposeFile {
    let mut services: Vec<ComposeService> = Vec::new();
    let mut in_services = false;
    for line in src.lines() {
        if line.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if !line.trim().is_empty() && indent == 0 {
            in_services = line.trim() == "services:";
            continue;
        }
        if !in_services {
            continue;
        }
        let text = line.trim();
        if indent == 2 {
            if let Some(name) = text.split(':').next() {
                services.push(ComposeService {
                    name: name.to_string(),
                    image: None,
                    ports: Vec::new(),
                });
            }
            continue;
        }
        if indent >= 4 && !services.is_empty() {
            let svc = services.last_mut().unwrap();
            if let Some(img) = text.strip_prefix("image:") {
                svc.image = Some(img.trim().trim_matches(|c| c == '"' || c == '\'').to_string());
            } else if let Some(p) = text.strip_prefix("- ") {
                let p = p.trim().trim_matches(|c| c == '"' || c == '\'');
                if p.contains(':') && p.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    svc.ports.push(p.to_string());
                }
            }
        }
    }
    ComposeFile {
        path: rel.to_string(),
        services,
    }
}