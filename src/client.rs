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

use crate::global::{BUFFER_SIZE, HTML_DEFAULT_V};
use crate::parse_req::{BodyMode, ChunkState, ParsedRequest, determine_body_mode, process_chunked};
use crate::setup::ListenerCtx;
use crate::utils::get_error_body;
use crate::{config, utils};
use crate::router::{self, ResponseCore, ResponseAction};

pub struct Client {
	pub fd: RawFd,
	pub listener_fd: RawFd,
	pub read_buf: Vec<u8>,
	pub write_buf: Vec<u8>,
	pub write_offset: usize,
	pub request: Option<ParsedRequest>,

	pub chunk_state: Option<ChunkState>,
    pub chunked_body: Vec<u8>,
}

#[derive(serde::Deserialize, Debug)]
struct CgiResponse {
	#[serde(default)]
    headers: HashMap<String, String>,
	#[serde(default)]
	error: Option<(u16, String)>,
	#[serde(default)]
    status: Option<u16>,
    body: String,
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
					chunk_state: None,
					chunked_body: Vec::new(),
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

			// Check the state of the request
			if client.request.is_none() {
				// Parser headers
				let mut headers = [httparse::EMPTY_HEADER; 64];
				let mut req_parse = httparse::Request::new(&mut headers);

				if let Ok(Status::Complete(h_len)) = req_parse.parse(&client.read_buf) {
					let body_mode = determine_body_mode(&req_parse);

					// Extract data before drain
					let method = req_parse.method.unwrap_or("").to_string();
					let path = req_parse.path.unwrap_or("").to_string();
					let version = req_parse.version.unwrap_or(HTML_DEFAULT_V);
					let headers_vec: Vec<(String, Vec<u8>)> = req_parse.headers.iter()
					.map(|h| (h.name.to_string(), h.value.to_vec()))
					.collect();

					// Comsuming only headers
					drop(req_parse);
					client.read_buf.drain(..h_len);

					match body_mode {
						BodyMode::None => {
							let parsed = ParsedRequest {
								method: Some(method),
								path: Some(path),
								version: Some(version),
								headers: Some(headers_vec),
								body: Vec::new(),
								body_mode,
							};
							client.request = Some(parsed);
							request_complete = true;
						}
						BodyMode::ContentLength(len) => {
							let parsed = ParsedRequest {
								method: Some(method),
								path: Some(path),
								version: Some(version),
								headers: Some(headers_vec),
								body: Vec::new(),
								body_mode,
							};
							client.request = Some(parsed);
						}
						BodyMode::Chunked => {
							let parsed = ParsedRequest {
								method: Some(method),
								path: Some(path),
								version: Some(version),
								headers: Some(headers_vec),
								body: Vec::new(),
								body_mode,
							};
							client.request = Some(parsed);
							client.chunk_state = Some(ChunkState::Size);
							client.chunked_body.clear();
						}
					}
				}
			}
			if let Some(req) = &mut client.request {
				match req.body_mode {
					BodyMode::ContentLength(len) => {
						if client.read_buf.len() >= len {
							req.body = client.read_buf.drain(..len).collect();
							request_complete = true;
						}
					}
					BodyMode::Chunked => {
						// Getting chunked_body full
						if let Some(ref mut chunk_state) = client.chunk_state {
							match process_chunked(chunk_state, &mut client.read_buf, &mut client.chunked_body) {
								Ok(done) => {
									if done {
										let headers_vec = req.headers.take().unwrap_or_default();
										let parsed = ParsedRequest {
											method: req.method.clone(),
											path: req.path.clone(),
											version: req.version,
											headers: Some(headers_vec),
											body: std::mem::take(&mut client.chunked_body),
											body_mode: BodyMode::Chunked,
										};
										client.request = Some(parsed);
										client.chunk_state = None;
										request_complete = true;
									}
								}
								Err(_) => {
									close_client(epoll_fd, fd, clients);
            						return;
								}
							}
						}
					}
					_ => {}
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
	let mut extra_headers: String = String::new();
	let mut final_status_code = resp.status_code;
    let mut final_status_text = resp.status_text;
	match resp.action {
		ResponseAction::ServeFile { path } => {
			body_bytes = match fs::read(path) {
				Ok(content) => content,
				Err(_) => (b"").to_vec(),
			}
		},
		ResponseAction::Error {server} => {
			body_bytes = get_error_body(resp.status_code, resp.status_text, server)
		},
		ResponseAction::Redirect { location } => {
			extra_headers.push_str(&format!("Location: {}\r\n", location));
		},
		ResponseAction::AutoIndex { dir } => {

		},
		ResponseAction::Cgi {
			interpreter,
			path,
			method,
			body,
			server,
			session,
		} => {
			let session = match session {
				Some(s) => s,
				None => String::new(),
			};
			let cgi_error_code = 500;
			let cgi_error_msg = "CGI Error";
			let cmd = Command::new(interpreter)
				.args([path, method, String::from_utf8(body).unwrap(), session])
				.output();
			match cmd {
				Ok(output) => {
					//println!("STDOUT : {:?}", String::from_utf8(output.stdout.clone()).unwrap());
					match serde_json::from_slice::<CgiResponse>(&output.stdout) {
						Ok(cgi_resp) => {
							if let Some((code, msg)) = cgi_resp.error {
								body_bytes = get_error_body(code, &msg, server);
								final_status_code = code;
                                final_status_text = "Error";
							} else {
								if let Some(status) = cgi_resp.status {
                                    final_status_code = status;
                                    final_status_text = match status {
                                        301 => "Moved Permanently",
                                        302 => "Found",
                                        303 => "See Other",
                                        _ => "OK",
                                    };
                                }
								for (key, value) in cgi_resp.headers {
									extra_headers.push_str(&format!("{}: {}\r\n", key, value))
								};
								body_bytes = cgi_resp.body.into_bytes();
							}
						}
						Err(_) => {
							eprintln!("Not Fnnuy");
							body_bytes = get_error_body(cgi_error_code, cgi_error_msg, server);
							final_status_code = cgi_error_code;
							final_status_text = cgi_error_msg;
						}
					}
				}
				Err(_) => {
					eprintln!("Oskour");
					body_bytes = get_error_body(cgi_error_code, cgi_error_msg, server);
							final_status_code = cgi_error_code;
							final_status_text = cgi_error_msg;
				}
			}
		}
	}

    let headers = format!(
        "HTTP/1.1 {} {}\r\n\
Content-Length: {}\r\n\
Content-Type: text/html; charset=UTF-8\r\n\
Connection: close\r\n\
{}\r\n",
        final_status_code,
        final_status_text,
        body_bytes.len(),
		extra_headers,
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