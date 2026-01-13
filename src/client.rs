//! All interactions with the client.

use std::collections::HashMap;
use std::net::{TcpListener};
use std::os::fd::IntoRawFd;
use std::os::unix::io::RawFd;
use std::fs;
use httparse::{Header, Status};
use libc::{
	EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD,
	EPOLLIN, EPOLLOUT, F_SETFL, O_NONBLOCK, epoll_ctl, epoll_event, fcntl
};

use crate::global::BUFFER_SIZE;
use crate::setup::ListenerCtx;
use crate::utils;
use crate::router::{self, Response};

pub struct Client {
	pub fd: RawFd,
	pub listener_fd: RawFd,
	pub read_buf: Vec<u8>,
	pub write_buf: Vec<u8>,
	pub write_offset: usize,
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
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

	loop {
		// Fill buffer and return the read bytes.
		let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, BUFFER_SIZE) };

		if n > 0 {
			// Fill client read buffer until the end of the header.
			client.read_buf.extend_from_slice(&buf[..n as usize]);

			// Detect end of the request header.
			if client.read_buf.windows(4).any(|w| w == b"\r\n\r\n") {
				let mut chunk = false;
				let read_buf_clone = client.read_buf.clone();
                let res = req.parse(&read_buf_clone).unwrap();
				utils::debug_print_request(&req, &res);

                if req.headers.iter().any(|h| h.name == "Transfer-Encoding" && str::from_utf8(h.value).unwrap() == "chunked") {
					chunk = true;
				}

				let mut header_offset = 0;

				match res {
					Status::Complete(i) => header_offset = i,
					Status::Partial => {},
				}

				let body = &read_buf_clone[header_offset..n as usize];

				let mut body_buf = &client.read_buf[header_offset..n as usize];
				let mut body_treated: Vec<u8> = Vec::new();

				if chunk {
					//treatment for chunked requests (maybe body isn't read properly, see later)
					// Les données arrivent en flux, il faut donc garder la connection ouverte et lire le(s) chunk(s) reçu et les concatener jusqu'a recevoir le chunk de fin (taille 0)
					// IMPORTANT :
					// chaque chunk ne créé pas de nouvelle requête
					println!("CHUNKED");
					let mut buf_index: usize = 0;
					// let mut chunk_info = httparse::parse_chunk_size(body_buf).unwrap();
					// while chunk_info.unwrap().1 != 0 {
					// 	buf_index = chunk_info.unwrap().0;
					// 	body_treated.append(body_buf[buf_index..(buf_index + chunk_info.unwrap().1 as usize)].to_vec().as_mut());
					// 	buf_index += chunk_info.unwrap().1 as usize;

					// 	chunk_info = httparse::parse_chunk_size(&body_buf[buf_index..]).unwrap()
					// }

					loop {
						body_buf = &client.read_buf[header_offset..];
						let chunk = httparse::parse_chunk_size(&body_buf[buf_index..]);
						match chunk {
							Ok(chunk_info) => {
								match chunk_info {
									Status::Complete((last_index, chunk_size)) => {
										if chunk_size == 0 {
											println!("END CHUNK RECEIVED");
											break
										} else {
											buf_index += last_index;
											println!("Chunk Info: {}, {}\nChunk: {}",last_index, chunk_size, String::from_utf8_lossy(&body_buf[(buf_index)..(buf_index+chunk_size as usize)]));
											body_treated.append(body_buf[(buf_index)..(buf_index+chunk_size as usize)].to_vec().as_mut());
											buf_index += chunk_size as usize + 2;
										}
									},
									Status::Partial => continue,
								}
							},
							Err(_e) => continue,
						}
					}

				} else {
					//treatment for not chunked requests
					println!("NOT CHUNKED");
					body_treated = body_buf.to_vec();
				}
				//test to see body, remove later
				println!("Body: {}", String::from_utf8_lossy(body_treated.as_slice()));

				client.read_buf.clear(); // Clear read buffer.
				let resp = router::router(listener_ctx, req, res, body); // Check & redirect to which handler/cgi we need, if not errors
				prepare_response(epoll_fd, fd, client, &resp);
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
fn prepare_response(epoll_fd: RawFd, fd: RawFd, client: &mut Client, resp: &Response) {
	let body_bytes: Vec<u8> = if let Some(p) = resp.path {
        match fs::read(p) {
            Ok(content) => content,
			Err(_) => resp.default_body.unwrap_or(b"").to_vec(),
        }
    } else {
        resp.default_body.unwrap_or(b"").to_vec()
    };

	let resp_headers = match &resp.headers {
		Some(h) =>
			h.iter()
			.map(|t| t.0.clone() + ": " + &(t.1))
			.reduce(|acc, s| acc + &s + "\r\n").unwrap(),
		None => String::new(),
	};

    let headers = format!(
        "HTTP/1.1 {} {}\r\n\
Content-Length: {}\r\n\
Content-Type: text/html; charset=UTF-8\r\n\
Connection: close\r\n
{}
\r\n",
        resp.status_code,
        resp.status_text,
        body_bytes.len(),
		resp_headers,
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
