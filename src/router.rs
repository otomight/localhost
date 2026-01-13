use std::path::PathBuf;

use httparse::{Request, Status};

use crate::{config::Route, global::{ERROR_400_HTML,ERROR_403_HTML,ERROR_404_HTML,ERROR_405_HTML,ERROR_413_HTML,ERROR_429_HTML,ERROR_500_HTML}, setup::ListenerCtx};

pub struct Response<'a> {
    pub status_code: u16,
    pub status_text: &'a str,
    pub headers: Option<Vec<(String, String)>>,     // When using redirect for Location
    pub path: Option<&'a str>,                      // Dynamic file
    pub default_body: Option<&'static [u8]>,        // Static error file if no dynamic file
}

pub fn router<'a>(listener_ctx: &ListenerCtx, req: Request, res: Status<usize>, body: &[u8]) -> Response<'a>{
    let req_path = match req.path {
        Some(p) => p,
        None => return error_400(), // Error 400
    };
    let req_method = match req.method {
        Some(m) => m,
        None => return error_400(), // Error 400
    };
    let host_header = match req.headers
    .iter()
    .find(|h| h.name.eq_ignore_ascii_case("Host")) {
        Some(h) => h,
        None => return error_400(), // Error 400
    };

    let host_str = match std::str::from_utf8(host_header.value) {
        Ok(s) => s,
        Err(_) => return error_400(), // Error 400
    };

    let (host, port) = match host_str.split_once(':') {
        Some((h, p)) => {
            let port = match p.parse::<u16>() {
                Ok(p) => p,
                Err(_) => return error_400(), // Error 400
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
        None => return error_404(), // Error 404
    };

    if server.client_max_body_size < body.len() {
        return error_413() // Error 413
    }

    let path = match server.routes.iter().filter(|r| req_path.starts_with(&r.path)).collect::<Vec<&Route>>().into_iter().max_by_key(|r| r.path.len()) {
        Some(r) => r,
        None => return error_404(), // Error 404
    };
    if !path.methods.iter().any(|m| m == req_method) {
        return error_405() // Error 405
    }
    if let Some(redirect) = path.redirect {
        // Redirect
        return redirect_301(redirect)
    }
    if path.cgi_extension != None && path.cgi_path != None {
        // CGI
    }

    if req_method == "GET" {
        let root = match path.root {
            Some(r) => r,
            None => return error_500(), // Error 500
        };

        let relative = &req_path[path.path.len()..];
        let relative = relative.trim_start_matches('/');

        let mut fs_path = PathBuf::from(root);
        fs_path.push(relative);

        if fs_path.is_file() {
            return Response {
                status_code: 200,
                status_text: "OK",
                headers: None,
                path: Some(fs_path.to_string_lossy().as_ref()),
                default_body: None,
            };
        }

        if fs_path.is_dir() {
            // Default page (index.html)
            if let Some(page) = path.page {
                let mut index_path = fs_path.clone();
                index_path.push(page);

                if index_path.is_file() {
                    return Response {
                        status_code: 200,
                        status_text: "OK",
                        headers: None,
                        path: Some(index_path.to_string_lossy().as_ref()),
                        default_body: None,
                    };
                }
            }
            // Dir without page
            if path.autoindex {
                let body = generate_autoindex(&fs_path);
                return Response {
                    status_code: 200,
                    status_text: "OK",
                    headers: None,
                    path: None,
                    default_body: Some(body),
                };
            }
            return error_403();

        }
    }
    return error_404()
}

// ----- Code functions -----

// Redirect Response
pub fn redirect_301<'a>(redirect: String) -> Response<'a> {
    return Response {
        status_code: 301,
        status_text: "Moved Permanently",
        headers: Some(vec![("Location".to_string(), redirect)]),
        path: None,
        default_body: None,
    }
}

// Error Response
pub fn error_400<'a>() -> Response<'a> {
    return Response {
        status_code: 400,
        status_text: "Bad Request",
        headers: None,
        path: None,
        default_body: Some(ERROR_400_HTML),
    }
}
pub fn error_403<'a>() -> Response<'a> {
    return Response {
        status_code: 403,
        status_text: "Forbidden",
        headers: None,
        path: None,
        default_body: Some(ERROR_403_HTML),
    }
}
pub fn error_404<'a>() -> Response<'a> {
    return Response {
        status_code: 404,
        status_text: "Not Found",
        headers: None,
        path: None,
        default_body: Some(ERROR_404_HTML),
    }
}
pub fn error_405<'a>() -> Response<'a> {
    return Response {
        status_code: 405,
        status_text: "Not Allowed",
        headers: None,
        path: None,
        default_body: Some(ERROR_405_HTML),
    }
}
pub fn error_413<'a>() -> Response<'a> {
    return Response {
        status_code: 413,
        status_text: "Payload Too Large",
        headers: None,
        path: None,
        default_body: Some(ERROR_413_HTML),
    }
}
pub fn error_429<'a>() -> Response<'a> {
    return Response {
        status_code: 429,
        status_text: "Too Many Request",
        headers: None,
        path: None,
        default_body: Some(ERROR_429_HTML),
    }
}
pub fn error_500<'a>() -> Response<'a> {
    return Response {
        status_code: 500,
        status_text: "Internal Server Error",
        headers: None,
        path: None,
        default_body: Some(ERROR_500_HTML),
    }
}