//! Server setup.

use std::{net::TcpListener, os::fd::{AsRawFd, RawFd}};

use libc::{EPOLL_CTL_ADD, EPOLLIN, epoll_create1, epoll_ctl, epoll_event};

use crate::config;

pub fn create_listeners(cfg: &config::ServerConfig) -> Vec<TcpListener> {
	let mut listeners = Vec::new();

	for port in &cfg.ports {
		let listener = TcpListener::bind((cfg.host.to_string(), *port))
			.expect("bind failed");

		listener.set_nonblocking(true).unwrap();
		println!("Listening on port {}", port);

		listeners.push(listener);
	}

	return listeners
}

/* Create Epoll and include the first 2 listeners of the server.
   Epoll will help managing all client requests.
   Primary features are adding client file descriptors, detecting incoming requests,
   managing their read and write rights and removing client at the end of the connection.
*/
pub fn setup_epoll(listeners: &[TcpListener]) -> RawFd {
	unsafe {
		let epoll_fd = epoll_create1(0);
		if epoll_fd < 0 {
			panic!("epoll_create1 failed");
		}

		for listener in listeners {
			let fd = listener.as_raw_fd();
			let mut event = epoll_event {
				events: EPOLLIN as u32,
				u64: fd as u64,
			};
			// Add new file descriptor to epoll with read acess.
			epoll_ctl(epoll_fd, EPOLL_CTL_ADD, fd, &mut event);
		}

		return epoll_fd
	}
}
