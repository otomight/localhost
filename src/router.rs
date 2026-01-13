use std::path::PathBuf;

use httparse::Request;

use crate::{config::{Route, ServerConfig}, setup::ListenerCtx};

pub enum ResponseAction<'a> {
    ServeFile { path: String },
    Redirect { location: String },
    AutoIndex { dir: String },
    Error {server: Option<&'a ServerConfig>},
    Cgi { script: String}
}

pub struct ResponseCore<'a> {
    pub status_code: u16,
    pub status_text: &'static str,
    pub action: ResponseAction<'a>,
}

pub fn router<'a>(listener_ctx: &'a ListenerCtx, req: Request, body: &[u8]) -> ResponseCore<'a> {
    let req_path = match req.path {
        Some(p) => p,
        None => return http_error(400, None), // Error 400
    };
    let req_method = match req.method {
        Some(m) => m,
        None => return http_error(400, None), // Error 400
    };
    let host_header = match req.headers
    .iter()
    .find(|h| h.name.eq_ignore_ascii_case("Host")) {
        Some(h) => h,
        None => return http_error(400, None), // Error 400
    };

    let host_str = match std::str::from_utf8(host_header.value) {
        Ok(s) => s,
        Err(_) => return http_error(400, None), // Error 400
    };

    let (host, port) = match host_str.split_once(':') {
        Some((h, p)) => {
            let port = match p.parse::<u16>() {
                Ok(p) => p,
                Err(_) => return http_error(400, None), // Error 400
            };
            (h, port)
        }
        None => {
            // No explicit port in header, take the one server sided from the listener
            (host_str, listener_ctx.listener.local_addr().unwrap().port())
        }
    };

    let server  = match listener_ctx.servers
    .iter()
    .find(|sc| {sc.server_name.as_deref() == Some(host) && sc.ports.contains(&port)}) {
        Some(s) => s,
        None => return http_error(404, None), // Error 404
    };

    if server.client_max_body_size < body.len() {
        return http_error(413, Some(server)) // Error 413
    }

    let path = match server.routes.iter().filter(|r| req_path.starts_with(&r.path)).collect::<Vec<&Route>>().into_iter().max_by_key(|r| r.path.len()) {
        Some(r) => r,
        None => return http_error(404, Some(server)), // Error 404
    };
    if !path.methods.iter().any(|m| m == req_method) {
        return http_error(405, Some(server)) // Error 405
    }
    // Redirect
    if let Some(redirect) = &path.redirect {
        return redirect_301(redirect)
    }
    // CGI
    if path.cgi_extension != None && path.cgi_path != None {
        return ResponseCore {
            status_code: 200,
            status_text: "OK",
            action: ResponseAction::Cgi { script: String::new() }
        };
    }

    if req_method == "GET" {
        let root = match &path.root {
            Some(r) => r,
            None => return http_error(500, Some(server)), // Error 500
        };

        let relative = &req_path[path.path.len()..];
        let relative = relative.trim_start_matches('/');

        let mut fs_path = PathBuf::from(root);
        fs_path.push(relative);

        if fs_path.is_file() {
            return ResponseCore {
                status_code: 200,
                status_text: "OK",
                action: ResponseAction::ServeFile { path: fs_path.to_string_lossy().as_ref().to_string() }
            };
        }

        if fs_path.is_dir() {
            // Default page (index.html)
            if let Some(page) = &path.page {
                let mut index_path = fs_path.clone();
                index_path.push(page);

                if index_path.is_file() {
                    return ResponseCore {
                        status_code: 200,
                        status_text: "OK",
                        action: ResponseAction::ServeFile { path: index_path.to_string_lossy().as_ref().to_string() },
                    };
                }
            }
            // Dir without page
            if path.autoindex {
                return ResponseCore {
                    status_code: 200,
                    status_text: "OK",
                    action: ResponseAction::AutoIndex { dir: fs_path.to_string_lossy().as_ref().to_string() }
                };
            }
            return http_error(403, Some(server))

        }
    }
    return http_error(404, Some(server))
}

// ----- Code functions -----

// Redirect Response
pub fn redirect_301<'a>(redirect: &str) -> ResponseCore<'a> {
    return ResponseCore {
        status_code: 301,
        status_text: "Moved Permanently",
        action: ResponseAction::Redirect { location: "Location: ".to_string() + redirect },
    }
}

// Error Response
pub fn http_error<'a>(code: u16, srvcfg: Option<&'a ServerConfig>) -> ResponseCore<'a> {
    let status_text = match code {
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Error",
    };

    ResponseCore {
        status_code: code,
        status_text,
        action: ResponseAction::Error { server: srvcfg },
    }
}