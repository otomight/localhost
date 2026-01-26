//! All interactions with the client.

use std::collections::HashMap;
use std::net::{TcpListener};
use std::os::fd::IntoRawFd;
use std::os::unix::io::RawFd;
use std::fs;
use std::process::Command;
use httparse::{Header, Status};
use libc::{
	EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD,
	EPOLLIN, EPOLLOUT, F_SETFL, O_NONBLOCK, epoll_ctl, epoll_event, fcntl
};

use crate::global::BUFFER_SIZE;
use crate::setup::ListenerCtx;
use crate::{config, utils};
use crate::router::{self, ResponseCore, ResponseAction};

pub enum BodyMode {
    None,
    ContentLength(usize),
    Chunked,
}

pub enum ChunkState {
    Size,
    Data(usize),
    CrLf,
    Done,
}

pub struct ParsedRequest {
	pub method: Option<String>,
    pub path: Option<String>,
    pub version: Option<u8>,
    pub headers: Option<Vec<(String, Vec<u8>)>>,
    pub body: Vec<u8>,
    pub body_mode: BodyMode,
}

pub struct Client {
	pub fd: RawFd,
	pub listener_fd: RawFd,
	pub read_buf: Vec<u8>,
	pub write_buf: Vec<u8>,
	pub write_offset: usize,
	pub request: Option<ParsedRequest>,
}

/// Set socket to non-blocking.
fn set_nonblocking(fd: RawFd) {
	unsafe {
		fcntl(fd, F_SETFL, O_NONBLOCK);
	}
}

/// Accept new incoming client connections.
pub fn handle_listener_event(
	epoll_fd: RawFd,
	listener_ctx: &ListenerCtx,
	listener_fd: RawFd,
	clients: &mut HashMap<RawFd, Client>,
) {
	loop {
		match listener_ctx.listener.accept() {
			Ok((stream, _)) => {
				// Transfer client stream ownership to a file descriptor.
				let fd = stream.into_raw_fd();
				set_nonblocking(fd);

				let mut event = epoll_event {
					events: EPOLLIN as u32,
					u64: fd as u64,
				};

				unsafe {
					// Add new client file descriptor to epoll.
					epoll_ctl(epoll_fd, EPOLL_CTL_ADD, fd, &mut event);
				}

				clients.insert(fd, Client {
					fd,
					listener_fd,
					read_buf: Vec::new(),
					write_buf: Vec::new(),
					write_offset: 0,
					request: None,
				});
			}
			Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
			Err(_) => break,
		}
	}
}


pub fn handle_client_read(
	epoll_fd: RawFd,
	fd: RawFd,
	clients: &mut HashMap<RawFd, Client>,
	listeners: &HashMap<RawFd, ListenerCtx>,
) {
	// Get client from hashmap table.
	let client = match clients.get_mut(&fd) {
		Some(c) => c,
		None => return,
	};
	let listener_ctx = match listeners.get(&client.listener_fd) {
		Some(l) => l,
		None => {
			close_client(epoll_fd, client.fd, clients);
			return;
		}
	};
	let mut buf = [0u8; BUFFER_SIZE];

	loop {
		// Fill buffer and return the read bytes.
		let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, BUFFER_SIZE) };

		if n > 0 {
			// Fill client read buffer until the end of the header.
			client.read_buf.extend_from_slice(&buf[..n as usize]);

			let mut request_complete = false;
			let mut headers = [httparse::EMPTY_HEADER; 64];
			let mut req = httparse::Request::new(&mut headers);

			// Check the state of the request
			if client.request.is_none() {
				if let Ok(Status::Complete(h_len)) = req.parse(&client.read_buf) {
					let body_mode = determine_body_mode(&req);

					let headers_vec = req.headers.iter()
					.map(|h| (h.name.to_string(), h.value.to_vec()))
					.collect::<Vec<_>>();

					let body = client.read_buf[h_len..].to_vec();

					let parsed = ParsedRequest {
						method: Some(req.method.unwrap_or("").to_string()),
						path: Some(req.path.unwrap_or("").to_string()),
						version: Some(req.version.unwrap_or(1)),
						headers: Some(headers_vec),
						body,
						body_mode,
					};

					let total_consumed = h_len + parsed.body.len();
					client.read_buf.drain(..total_consumed);
					client.request = Some(parsed);
				}
			}
			if let Some(req) = &mut client.request {
				match req.body_mode {
					BodyMode::None => {
						request_complete = true;
					}

					BodyMode::ContentLength(expected) => {
						if req.body.len() >= expected {
							req.body.truncate(expected);
							request_complete = true;
						}
					}

					BodyMode::Chunked => {
						// PAS ICI (machine d’état séparée)
					}
				}
			}
			if request_complete && let Some(parsed) = client.request.take() {
				let resp = router::router(listener_ctx, parsed); // Give back where and what to do
				prepare_response(epoll_fd, fd, client, resp);
				client.request = None;
				break;
			}
		} else if n == 0 {
			close_client(epoll_fd, fd, clients);
			break;
		} else {
			let err = unsafe { *libc::__errno_location() };
			if err == libc::EAGAIN || err == libc::EWOULDBLOCK {
				break;
			} else {
				close_client(epoll_fd, fd, clients);
				break;
			}
		}
	}
}

pub fn handle_client_write(
	epoll_fd: RawFd,
	fd: RawFd,
	clients: &mut HashMap<RawFd, Client>,
) {
	let client = match clients.get_mut(&fd) {
		Some(c) => c,
		None => return,
	};

	while client.write_offset < client.write_buf.len() {
		let n = unsafe {
			libc::write(
				fd,
				client.write_buf[client.write_offset..].as_ptr() as *const _,
				client.write_buf.len() - client.write_offset,
			)
		};

		println!("response written");

		if n > 0 {
			client.write_offset += n as usize;
		} else {
			let err = unsafe { *libc::__errno_location() };
			if err == libc::EAGAIN || err == libc::EWOULDBLOCK {
				return;
			}
			break;
		}
	}

	close_client(epoll_fd, fd, clients);
}

/// Write the buffer of the client and set its epoll config with write access.
/// The response will be treated by epoll at the next event check in the main loop.
fn prepare_response(epoll_fd: RawFd, fd: RawFd, client: &mut Client, resp: ResponseCore) {
	let mut body_bytes: Vec<u8> = Vec::new();
	let mut header_location: String = String::new();
	match resp.action {
		ResponseAction::ServeFile { path } => {
			body_bytes = match fs::read(path) {
				Ok(content) => content,
				Err(_) => (b"").to_vec(),
			}
		},
		ResponseAction::Error {server} => {
			let code = resp.status_code;
			// If config && path, serve custom errors (errors/4xx.html), else minimal fallback
			if let Some(cfg) = server {
				if let Some(path) = cfg.error_pages.get(&code) {
					if let Ok(bytes) = fs::read(path) {
						body_bytes = bytes;
					}
				}
			} else {
				body_bytes = format!(
					"<html><head><title>{0} {1}</title></head>\
					<body><h1>{0} {1}</h1></body></html>",
					code, resp.status_text
				).into_bytes();
			}
		},
		ResponseAction::Redirect { location } => {
			header_location = format!("Location: {}\r\n", location);
		},
		ResponseAction::AutoIndex { dir } => {

		},
		ResponseAction::Cgi {
			interpreter,
			path,
			method,
			body,
		} => {
			let cmd = Command::new("exec")
				.args([interpreter, path, method, body])
				.output()
				.expect("ERREUR EXECUTION CGI");
			body_bytes = cmd.stdout
		},
	}

    let headers = format!(
        "HTTP/1.1 {} {}\r\n\
Content-Length: {}\r\n\
Content-Type: text/html; charset=UTF-8\r\n\
Connection: close\r\n\
{}\r\n",
        resp.status_code,
        resp.status_text,
        body_bytes.len(),
		header_location,
    );

    client.write_buf = headers.into_bytes();
    client.write_buf.extend_from_slice(&body_bytes);


	let mut event = epoll_event {
		events: EPOLLOUT as u32,
		u64: fd as u64,
	};

	unsafe {
		// Modify the client epoll config with write access.
		epoll_ctl(epoll_fd, EPOLL_CTL_MOD, fd, &mut event);
	}
}

pub fn close_client(
	epoll_fd: RawFd,
	fd: RawFd,
	clients: &mut HashMap<RawFd, Client>,
) {
	clients.remove(&fd);
	unsafe {
		// Remove the client file descriptor from epoll.
		epoll_ctl(epoll_fd, EPOLL_CTL_DEL, fd, std::ptr::null_mut());
		libc::shutdown(fd, libc::SHUT_WR);
		libc::close(fd);
	}
}

// Determine the body_mode of the http request
fn determine_body_mode(req: &httparse::Request) -> BodyMode {
    for h in req.headers.iter() {
        if h.name.eq_ignore_ascii_case("Transfer-Encoding")
            && h.value.eq_ignore_ascii_case(b"chunked")
        {
            return BodyMode::Chunked;
        }
    }

    for h in req.headers.iter() {
        if h.name.eq_ignore_ascii_case("Content-Length") {
            if let Ok(len) = std::str::from_utf8(h.value).unwrap().parse::<usize>() {
                return BodyMode::ContentLength(len);
            }
        }
    }

    BodyMode::None
}