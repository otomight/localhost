//! Utilitaries functions.

use std::fs;
use crate::{config::ServerConfig, parse_req::ParsedRequest};

pub fn debug_print_request(request: &httparse::Request, result: &httparse::Status<usize>) {
    println!("\n--- HTTP REQUEST BEGIN ---");
	println!("Method: {:?}", request.method);
    println!("Path: {:?}", request.path);
    println!("Version: {:?}", request.version);
    for header in request.headers.iter() {
        println!("Header: {}: {}", header.name, String::from_utf8_lossy(header.value));
    }
    println!("Request is {}", if result.is_complete() { "complete" } else { "partial" });
    println!("--- HTTP REQUEST END ---\n");
}

// If config && path, serve custom errors (4xx/5xx.html), else minimal fallback
pub fn get_error_body(code: u16, status_text: &str, server: Option<&ServerConfig>) -> Vec<u8> {
    // Try custom error page first
    if let Some(cfg) = server {
        if let Some(path) = cfg.error_pages.get(&code) {
            if let Ok(bytes) = fs::read(path) {
                return bytes;
            }
        }
    }
    // Fallback
    format!(
        "<html><head><title>{0} {1}</title></head>\
        <body><h1>{0} {1}</h1></body></html>",
        code, status_text
    ).into_bytes()
}

pub fn get_cookie(req: &ParsedRequest, name: &str) -> Option<String> {
    let headers = req.headers.as_ref()?;

    for (k, v) in headers {
        if k.eq_ignore_ascii_case("cookie") {
            let s = std::str::from_utf8(v).ok()?;
            for part in s.split(';') {
                let mut it = part.trim().splitn(2, '=');
                if it.next()? == name {
                    return Some(it.next()?.to_string());
                }
            }
        }
    }
    None
}
