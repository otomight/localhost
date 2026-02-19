use std::{path::PathBuf, str::{from_utf8}};

use crate::{config::{Route, ServerConfig}, parse_req::ParsedRequest, setup::ListenerCtx, utils::get_cookie};

pub enum ResponseAction<'a> {
    ServeFile { path: String },
    Redirect { location: String },
    AutoIndex { dir: String,
        path: String
    },
    Error {server: Option<&'a ServerConfig>},
    Cgi { interpreter:String,
        path: String,
        method: String,
        body: Vec<u8>,
        server: Option<&'a ServerConfig>,
        session: Option<String>
    },
    Upload { body: Vec<u8>,
        content_type: String
    },
}

pub struct ResponseCore<'a> {
    pub status_code: u16,
    pub status_text: &'static str,
    pub action: ResponseAction<'a>,
}

pub fn router<'a>(listener_ctx: &'a ListenerCtx, req: ParsedRequest) -> ResponseCore<'a> {
    let req_path = match req.path {
        Some(ref p) => p,
        None => return http_error(400, None), // Error 400
    };
    let req_method = match req.method {
        Some(ref m) => m,
        None => return http_error(400, None), // Error 400
    };
    let headers = match &req.headers {
        Some(h) => h,
        None => return http_error(400, None), // Error 400
    };
    let host_header = headers
    .iter()
    .find(|(key, _)| key.eq_ignore_ascii_case("host"));

    let host_str = match host_header {
        Some((_, value)) => match std::str::from_utf8(value) {
            Ok(s) => s,
            Err(_) => return http_error(400, None), // Error 400 (no value for host)
        },
        None => return http_error(400, None), // Error 400 (no host header)
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

    let session_id = get_cookie(&req, "session");

    let server  = match listener_ctx.servers
    .iter()
    .find(|sc| {sc.server_name.as_deref() == Some(host) && sc.ports.contains(&port)}) {
        Some(s) => s,
        None => return http_error(404, None), // Error 404
    };

    if server.client_max_body_size < req.body.len() {
        return http_error(413, Some(server)) // Error 413
    }

    let path = match server.routes.iter().filter(|r| req_path.starts_with(&r.path)).collect::<Vec<&Route>>().into_iter().max_by_key(|r| r.path.len()) {
        Some(r) => r,
        None => return http_error(404, Some(server)), // Error 404
    };
    if !path.methods.iter().any(|m| m == req_method) {
        return http_error(405, Some(server)) // Error 405
    }
    let mut ext = false;
    if let Some(v) = &path.cgi_extension {
        if req_path.ends_with(v) {
            ext = true;
        }
    }

    // Redirect
    if let Some(redirect) = &path.redirect {
        return redirect_301(redirect)
    }

    // Upload
    let ctype_val = headers.iter()
    .find(|(key, _)| key.eq_ignore_ascii_case("Content-Type"))
        .map(|(_, value)| value.as_slice());

    if let Some(v) = ctype_val {
        if let Ok(s) = from_utf8(v) {
            if s.starts_with("multipart/form-data") {
                return ResponseCore {
                    status_code: 201,
                    status_text: "Created",
                    action: ResponseAction::Upload {
                        body: req.body,
                        content_type: s.to_string(),
                    },
                };
            }
        }
    }

    // CGI
    let relative = &req_path[path.path.len()..].trim_start_matches('/'); // Slicing to obtain "script.py"
    let mut script_path = PathBuf::from(path.root.as_ref().unwrap());
    script_path.push(relative);  // "cgi-bin/script.py"

    if path.cgi_extension.is_some() && path.cgi_path.is_some() && ext {
        return ResponseCore {
            status_code: 200,
            status_text: "OK",
            action: ResponseAction::Cgi {
                interpreter: path.cgi_path.clone().unwrap(),
                path: script_path.to_string_lossy().to_string(),
                method: req_method.to_string(),
                body: req.body,
                server: Some(server),
                session: session_id,
            }
        };
    }

    // Files and Dirs
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
                    action: ResponseAction::AutoIndex { dir: fs_path.to_string_lossy().as_ref().to_string(), path: path.path.clone() }
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
        action: ResponseAction::Redirect { location: redirect.to_string() },
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