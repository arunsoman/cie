use cie_server::http::parse;
#[test]
fn parser_rejects_request_smuggling_shapes() {
    for raw in ["POST /api/questions HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nContent-Length: 2\r\n\r\n", "POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n", "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: -1\r\n\r\n", "GET / HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\n\r\nx"]{assert!(parse(&mut raw.as_bytes()).is_err());}
}
#[test]
fn parser_requires_bounded_complete_bodies() {
    let raw = "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 999999999\r\n\r\n";
    assert_eq!(parse(&mut raw.as_bytes()).err().unwrap().status, 413);
    assert!(
        parse(&mut b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\n{}".as_slice())
            .is_err()
    );
    assert_eq!(
        parse(&mut b"POST / HTTP/1.1\r\nHost: x\r\n\r\n".as_slice())
            .err()
            .unwrap()
            .status,
        411
    );
}
#[test]
fn parser_accepts_exact_json_request() {
    let req=parse(&mut b"POST /api/search HTTP/1.1\r\nHost: localhost:4317\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}".as_slice()).unwrap();
    assert_eq!(req.method, "POST");
    assert_eq!(req.body, b"{}");
}

fn exchange(address: std::net::SocketAddr, request: &str) -> String {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).unwrap();
    out
}
#[test]
fn live_transport_enforces_origin_host_session_and_security_headers() {
    use std::sync::atomic::Ordering;
    let dir = std::env::temp_dir().join(format!("cie-http-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let app = cie_server::api::App::open(&dir.join("data"), vec![dir.clone()]).unwrap();
    let server = cie_server::http::Server::bind(([127, 0, 0, 1], 0).into(), app).unwrap();
    let addr = server.address;
    let stop = server.stop.clone();
    let thread = std::thread::spawn(move || server.run().unwrap());
    let root = exchange(addr, &format!("GET / HTTP/1.1\r\nHost: {addr}\r\n\r\n"));
    assert!(root.starts_with("HTTP/1.1 200"));
    assert!(root.contains("frame-ancestors 'none'") && root.contains("Cache-Control: no-store"));
    let token = root
        .split("name=\"cie-token\" content=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let unauthorized = exchange(
        addr,
        &format!("GET /api/state HTTP/1.1\r\nHost: {addr}\r\n\r\n"),
    );
    assert!(unauthorized.starts_with("HTTP/1.1 401"));
    let evil=exchange(addr,&format!("GET /api/state HTTP/1.1\r\nHost: {addr}\r\nOrigin: https://evil.example\r\nX-Cie-Token: {token}\r\n\r\n"));
    assert!(evil.starts_with("HTTP/1.1 403"));
    let rebind = exchange(addr, "GET / HTTP/1.1\r\nHost: evil.example\r\n\r\n");
    assert!(rebind.starts_with("HTTP/1.1 403"));
    let authenticated = exchange(
        addr,
        &format!("GET /api/state HTTP/1.1\r\nHost: {addr}\r\nX-Cie-Token: {token}\r\n\r\n"),
    );
    assert!(authenticated.starts_with("HTTP/1.1 200"));
    assert!(authenticated.contains("\"cloudEnabled\":false"));
    stop.store(true, Ordering::SeqCst);
    thread.join().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
