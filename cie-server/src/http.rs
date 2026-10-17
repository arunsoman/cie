//! Minimal bounded HTTP/1.1 transport, deliberately restricted to loopback.
use crate::{
    api::App,
    model::{Error, Result},
};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
pub fn parse(reader: &mut impl Read) -> Result<Request> {
    let mut header = vec![];
    let mut byte = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 16384 {
            return Err(Error::new(
                431,
                "HEADER_LIMIT",
                "Request headers exceed their size limit",
            ));
        }
        reader
            .read_exact(&mut byte)
            .map_err(|_| Error::new(400, "INCOMPLETE_REQUEST", "Request headers are incomplete"))?;
        header.push(byte[0]);
    }
    let text = std::str::from_utf8(&header)
        .map_err(|_| Error::new(400, "INVALID_HTTP", "Headers must be UTF-8"))?;
    let mut lines = text.split("\r\n");
    let first: Vec<_> = lines.next().unwrap_or_default().split(' ').collect();
    if first.len() != 3
        || first[2] != "HTTP/1.1"
        || !["GET", "POST", "DELETE"].contains(&first[0])
        || !first[1].starts_with('/')
        || first[1].contains(['\r', '\n', '\0'])
    {
        return Err(Error::new(400, "INVALID_HTTP", "Unsupported request line"));
    }
    let mut headers = BTreeMap::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| Error::new(400, "INVALID_HEADER", "Malformed header"))?;
        if k.is_empty()
            || !k.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || v.contains(['\r', '\n', '\0'])
        {
            return Err(Error::new(400, "INVALID_HEADER", "Malformed header"));
        }
        if headers
            .insert(k.to_ascii_lowercase(), v.trim().to_string())
            .is_some()
        {
            return Err(Error::new(
                400,
                "DUPLICATE_HEADER",
                "Duplicate headers are unsupported",
            ));
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err(Error::new(
            400,
            "UNSUPPORTED_ENCODING",
            "Chunked requests are not supported",
        ));
    }
    if first[0] == "POST" && !headers.contains_key("content-length") {
        return Err(Error::new(
            411,
            "LENGTH_REQUIRED",
            "Content-Length required",
        ));
    }
    let length = match headers.get("content-length") {
        Some(s) if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) => s
            .parse::<usize>()
            .map_err(|_| Error::new(413, "BODY_LIMIT", "Body size exceeds its limit"))?,
        Some(_) => return Err(Error::new(400, "INVALID_LENGTH", "Invalid Content-Length")),
        None => 0,
    };
    if length > 1024 * 1024 {
        return Err(Error::new(413, "BODY_LIMIT", "Request body exceeds 1 MiB"));
    }
    if first[0] != "POST" && length != 0 {
        return Err(Error::new(
            400,
            "UNEXPECTED_BODY",
            "This method must not have a body",
        ));
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|_| Error::new(400, "INCOMPLETE_BODY", "Request body is incomplete"))?;
    Ok(Request {
        method: first[0].into(),
        target: first[1].into(),
        headers,
        body,
    })
}
fn same_secret(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}
pub struct Server {
    pub address: SocketAddr,
    pub stop: Arc<AtomicBool>,
    listener: TcpListener,
    app: Arc<App>,
    token: String,
}
impl Server {
    pub fn bind(address: SocketAddr, app: Arc<App>) -> Result<Self> {
        if !address.ip().is_loopback() {
            return Err(Error::new(
                400,
                "LOOPBACK_REQUIRED",
                "Only loopback binding is supported",
            ));
        }
        let listener = TcpListener::bind(address)?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let mut random = [0u8; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let token = random.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self {
            address,
            stop: Arc::new(AtomicBool::new(false)),
            listener,
            app,
            token,
        })
    }
    pub fn run(self) -> Result<()> {
        let active = Arc::new(AtomicUsize::new(0));
        while !self.stop.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((mut stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    if active.fetch_add(1, Ordering::SeqCst) >= 16 {
                        active.fetch_sub(1, Ordering::SeqCst);
                        let _ = write_response(
                            &mut stream,
                            503,
                            "application/json",
                            b"{\"error\":{\"code\":\"BUSY\"}}",
                        );
                        continue;
                    }
                    let app = Arc::clone(&self.app);
                    let token = self.token.clone();
                    let address = self.address;
                    let count = Arc::clone(&active);
                    std::thread::spawn(move || {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                        handle(&mut stream, &app, address, &token);
                        count.fetch_sub(1, Ordering::SeqCst);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
fn handle(stream: &mut TcpStream, app: &Arc<App>, address: SocketAddr, token: &str) {
    let result = (|| -> Result<(&'static str, Vec<u8>)> {
        let req = parse(stream)?;
        let host = req
            .headers
            .get("host")
            .ok_or_else(|| Error::new(403, "HOST_REQUIRED", "Valid local Host header required"))?;
        if host != &address.to_string() && host != &format!("localhost:{}", address.port()) {
            return Err(Error::new(
                403,
                "INVALID_HOST",
                "Host is not this local workspace",
            ));
        }
        if req
            .headers
            .get("origin")
            .is_some_and(|origin| origin != &format!("http://{host}"))
        {
            return Err(Error::new(
                403,
                "INVALID_ORIGIN",
                "Cross-origin access is not permitted",
            ));
        }
        if req.method == "GET" {
            match req.target.as_str() {
                "/" | "/index.html" => {
                    return Ok((
                        "text/html; charset=utf-8",
                        include_str!("../web/index.html")
                            .replace("__CIE_TOKEN__", token)
                            .into_bytes(),
                    ))
                }
                "/app.js" => {
                    return Ok((
                        "text/javascript; charset=utf-8",
                        include_bytes!("../web/app.js").to_vec(),
                    ))
                }
                "/style.css" => {
                    return Ok((
                        "text/css; charset=utf-8",
                        include_bytes!("../web/style.css").to_vec(),
                    ))
                }
                "/api/health" => {
                    return Ok((
                        "application/json",
                        serde_json::to_vec(&app.dispatch("GET", "/api/health", &[])?).unwrap(),
                    ))
                }
                _ => {}
            }
        }
        if !req
            .headers
            .get("x-cie-token")
            .is_some_and(|value| same_secret(value, token))
        {
            return Err(Error::new(
                401,
                "SESSION_REQUIRED",
                "Session expired or missing. Reload the local workspace.",
            ));
        }
        if req.method == "POST"
            && !req
                .headers
                .get("content-type")
                .is_some_and(|s| s.split(';').next() == Some("application/json"))
        {
            return Err(Error::new(415, "JSON_REQUIRED", "Use application/json"));
        }
        Ok((
            "application/json; charset=utf-8",
            serde_json::to_vec(&app.dispatch(&req.method, &req.target, &req.body)?)
                .map_err(|_| Error::new(500, "SERIALIZE", "Cannot serialize response"))?,
        ))
    })();
    match result {
        Ok((mime, bytes)) => {
            let _ = write_response(stream, 200, mime, &bytes);
        }
        Err(e) => {
            let bytes = serde_json::to_vec(
                &serde_json::json!({"error":{"code":e.code,"message":e.message}}),
            )
            .unwrap_or_default();
            let _ = write_response(stream, e.status, "application/json; charset=utf-8", &bytes);
        }
    }
}
fn write_response(
    stream: &mut TcpStream,
    status: u16,
    mime: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write!(stream,"HTTP/1.1 {status} Response\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'self'\r\n\r\n",body.len())?;
    stream.write_all(body)?;
    stream.flush()
}
