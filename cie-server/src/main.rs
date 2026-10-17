use std::path::PathBuf;
fn main() {
    if let Err(issue) = run() {
        eprintln!("{}: {}", issue.code, issue.message);
        std::process::exit(1)
    }
}
fn run() -> cie_server::model::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut roots = vec![];
    let mut directory = PathBuf::from(".cie-workspace");
    let mut port = 4317;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--allow-root" => roots.push(PathBuf::from(args.next().ok_or_else(usage)?)),
            "--data" => directory = PathBuf::from(args.next().ok_or_else(usage)?),
            "--port" => {
                port = args
                    .next()
                    .ok_or_else(usage)?
                    .parse::<u16>()
                    .map_err(|_| usage())?
            }
            "--help" => {
                println!("cie-workspace [--allow-root PATH]... [--data PATH] [--port PORT]\nLocal single-user workspace; no model egress, remote execution or source mutation.");
                return Ok(());
            }
            _ => return Err(usage()),
        }
    }
    if roots.is_empty() {
        roots.push(std::env::current_dir()?);
    }
    let app = cie_server::api::App::open(&directory, roots)?;
    let server = cie_server::http::Server::bind(([127, 0, 0, 1], port).into(), app)?;
    println!(
        "CIE workspace: http://{}\nLocal single-user mode. Cloud model egress disabled.",
        server.address
    );
    server.run()
}
fn usage() -> cie_server::model::Error {
    cie_server::model::Error::new(400, "USAGE", "Use cie-workspace --help")
}
