//! Serve only explicitly selected videos over a token-protected loopback
//! stream. Linux WebKit cannot open Tauri's custom schemes at all, and where
//! it can, the asset protocol hands out about a megabyte per request: far
//! short of the 30 MB a second a camera's 10-bit 4K footage needs.
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex, OnceLock},
};
type Files = Arc<Mutex<HashMap<String, PathBuf>>>;
struct Server {
    port: u16,
    secret: String,
    files: Files,
}
static SERVER: OnceLock<Result<Server, String>> = OnceLock::new();

pub fn url(path: PathBuf) -> Result<String, String> {
    let server = SERVER.get_or_init(start).as_ref().map_err(Clone::clone)?;
    let key = format!(
        "{:x}",
        xxhash_rust::xxh3::xxh3_64(path.to_string_lossy().as_bytes())
    );
    server
        .files
        .lock()
        .map_err(|e| e.to_string())?
        .insert(key.clone(), path);
    Ok(format!(
        "http://127.0.0.1:{}/{}/{key}",
        server.port, server.secret
    ))
}

fn start() -> Result<Server, String> {
    let listener =
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let secret: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let files: Files = Default::default();
    let (send, receive) = mpsc::sync_channel::<TcpStream>(32);
    let receive = Arc::new(Mutex::new(receive));
    // A player holds a long request open while it buffers, and drops it to seek.
    for n in 0..16 {
        let (receive, files, secret) = (receive.clone(), files.clone(), secret.clone());
        std::thread::Builder::new()
            .name(format!("opencull-video-{n}"))
            .spawn(move || loop {
                let stream = receive.lock().ok().and_then(|r| r.recv().ok());
                let Some(mut stream) = stream else { break };
                let _ = respond(&mut stream, &files, &secret);
            })
            .map_err(|e| e.to_string())?;
    }
    std::thread::Builder::new()
        .name("opencull-video-listener".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = send.try_send(stream);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Server {
        port,
        secret,
        files,
    })
}

fn range(value: Option<&str>, len: u64) -> Option<(u64, u64)> {
    let value = value?.strip_prefix("bytes=")?;
    if len == 0 || value.contains(',') {
        return None;
    }
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        let count: u64 = end.parse().ok()?;
        if count == 0 {
            return None;
        }
        return Some((len.saturating_sub(count), len - 1));
    }
    let start = start.parse::<u64>().ok()?;
    let end = if end.is_empty() {
        len - 1
    } else {
        end.parse::<u64>().ok()?.min(len - 1)
    };
    (start < len && end >= start).then_some((start, end))
}
fn error(stream: &mut TcpStream, status: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}
fn respond(stream: &mut TcpStream, files: &Files, secret: &str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(15)))?;
    let mut header = Vec::new();
    let mut byte = [0];
    while header.len() < 8192 && !header.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Ok(());
        }
        header.push(byte[0]);
    }
    if !header.ends_with(b"\r\n\r\n") {
        return error(stream, "431 Request Header Fields Too Large");
    }
    let text = String::from_utf8_lossy(&header);
    let mut lines = text.split("\r\n");
    let mut request = lines.next().unwrap_or_default().split_whitespace();
    let method = request.next().unwrap_or_default();
    let uri = request.next().unwrap_or_default();
    if method != "GET" && method != "HEAD" {
        return error(stream, "405 Method Not Allowed");
    }
    let prefix = format!("/{secret}/");
    let Some(key) = uri
        .split('?')
        .next()
        .unwrap_or_default()
        .strip_prefix(&prefix)
    else {
        return error(stream, "404 Not Found");
    };
    let path = files.lock().ok().and_then(|f| f.get(key).cloned());
    let Some(path) = path else {
        return error(stream, "404 Not Found");
    };
    let Ok(mut file) = File::open(&path) else {
        return error(stream, "404 Not Found");
    };
    let len = file.metadata()?.len();
    let wanted = lines.find_map(|line| {
        line.split_once(':')
            .filter(|(k, _)| k.eq_ignore_ascii_case("range"))
            .map(|(_, v)| v.trim())
    });
    let (start, end) = if wanted.is_some() {
        let Some(bounds) = range(wanted, len) else {
            return write!(stream, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        };
        bounds
    } else {
        (0, len.saturating_sub(1))
    };
    let count = if len == 0 { 0 } else { end - start + 1 };
    let mime = match crate::raw::extension(&path).as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mts" | "m2ts" => "video/mp2t",
        "avi" => "video/x-msvideo",
        "3gp" => "video/3gpp",
        _ => "video/mpeg",
    };
    let status = if wanted.is_some() {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {count}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n")?;
    if wanted.is_some() {
        write!(stream, "Content-Range: bytes {start}-{end}/{len}\r\n")?;
    }
    write!(stream, "\r\n")?;
    if method == "GET" {
        file.seek(SeekFrom::Start(start))?;
        std::io::copy(&mut file.take(count), stream)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_and_invalid_ranges() {
        assert_eq!(range(Some("bytes=2-5"), 10), Some((2, 5)));
        assert_eq!(range(Some("bytes=7-"), 10), Some((7, 9)));
        assert_eq!(range(Some("bytes=-3"), 10), Some((7, 9)));
        assert_eq!(range(Some("bytes=0-999"), 10), Some((0, 9)));
        for value in ["bytes=10-", "bytes=5-2", "bytes=-0", "bytes=1-2,4-5", "bad"] {
            assert_eq!(range(Some(value), 10), None);
        }
    }
    #[test]
    fn loopback_requires_token_and_serves_selected_byte_ranges() {
        let path = std::env::temp_dir().join(format!("opencull-stream-{}.mp4", std::process::id()));
        std::fs::write(&path, b"0123456789").unwrap();
        let files = Arc::new(Mutex::new(HashMap::from([("file".into(), path.clone())])));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                respond(&mut stream, &files, "secret").unwrap();
            }
        });
        let request = |url: &str, range: &str| {
            let mut stream = TcpStream::connect(addr).unwrap();
            write!(
                stream,
                "GET {url} HTTP/1.1\r\nHost: localhost\r\n{range}\r\n"
            )
            .unwrap();
            let mut out = String::new();
            stream.read_to_string(&mut out).unwrap();
            out
        };
        let partial = request("/secret/file", "Range: bytes=2-5\r\n");
        assert!(partial.starts_with("HTTP/1.1 206"));
        assert!(partial.ends_with("2345"));
        assert!(request("/wrong/file", "").starts_with("HTTP/1.1 404"));
        assert!(request("/secret/file", "Range: bytes=99-\r\n").starts_with("HTTP/1.1 416"));
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
