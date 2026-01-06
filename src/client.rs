//! All interactions with the client.

use std::collections::HashMap;
use std::net::{TcpListener};
use std::os::fd::IntoRawFd;
use std::os::unix::io::RawFd;
use libc::{
	EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD,
	EPOLLIN, EPOLLOUT, F_SETFL, O_NONBLOCK, epoll_ctl, epoll_event, fcntl
};

use crate::global::BUFFER_SIZE;
use crate::utils;

pub struct Client {
	pub fd: RawFd,
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
	listener: &TcpListener,
	clients: &mut HashMap<RawFd, Client>,
) {
	loop {
		match listener.accept() {
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
) {
	// Get client from hashmap table.
	let client = match clients.get_mut(&fd) {
		Some(c) => c,
		None => return,
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
                let read_buf_clone = client.read_buf.clone();
                let res = req.parse(&read_buf_clone).unwrap();
                utils::debug_print_request(&req, &res);

				client.read_buf.clear(); // Clear read buffer.
				prepare_response(epoll_fd, fd, client, &req, res);
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
fn prepare_response(
    epoll_fd: RawFd,
    fd: RawFd,
    client: &mut Client,
    request: &httparse::Request,
    result: httparse::Status<usize>
) {
	let body = b"<html><body><h1>It works</h1></body></html>";

	let headers = format!(
		"HTTP/1.1 200 OK\r\n\
Content-Length: {}\r\n\
Content-Type: text/html\r\n\
Connection: close\r\n\r\n",
		body.len()
	);

	client.write_buf = headers.into_bytes();
	client.write_buf.extend_from_slice(body);

	let mut event = epoll_event {
		events: EPOLLOUT as u32,
		u64: fd as u64,
	};

	unsafe {
		// Modify the client epoll config with write access.
		epoll_ctl(epoll_fd, EPOLL_CTL_MOD, fd, &mut event);
	}
}

fn close_client(
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
