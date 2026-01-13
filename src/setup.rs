//! Server setup.

use std::{collections::HashMap, net::TcpListener, os::fd::{AsRawFd, RawFd}};

use libc::{EPOLL_CTL_ADD, EPOLLIN, epoll_create1, epoll_ctl, epoll_event};

use crate::config::{self, ServerConfig};

#[derive(Debug)]
pub struct ListenerCtx {
	pub listener: TcpListener,
	pub servers: Vec<ServerConfig>
}

pub fn create_listeners(cfg: &config::Config) -> HashMap<RawFd, ListenerCtx> {
	let mut map: HashMap<(String, u16), Vec<ServerConfig>>
		= HashMap::new();

	// Group servers by (host, port).
	for server in &cfg.servers {
		for port in &server.ports {
			map.entry((server.host.clone(), *port))
				.or_insert(Vec::new())
				.push(server.clone());
		}
	}

	// Create ONE listener per (host, port).
	let mut listeners = HashMap::new();

	for ((host, port), servers) in map {
		let listener = TcpListener::bind((host.as_str(), port))
			.expect("bind failed");

		listener.set_nonblocking(true).unwrap();
		println!("Listening on port {}", port);

		listeners.insert(listener.as_raw_fd(), ListenerCtx {
			listener,
			servers,
		});
	}

	return listeners
}

/* Create Epoll and include the first 2 listeners of the server.
   Epoll will help managing all client requests.
   Primary features are adding client file descriptors, detecting incoming requests,
   managing their read and write rights and removing client at the end of the connection.
*/
pub fn setup_epoll(listeners: &HashMap<RawFd, ListenerCtx>) -> RawFd {
	unsafe {
		let epoll_fd = epoll_create1(0);
		if epoll_fd < 0 {
			panic!("epoll_create1 failed");
		}

		for listener in listeners {
			let mut event = epoll_event {
				events: EPOLLIN as u32,
				u64: *listener.0 as u64,
			};
			// Add new file descriptor to epoll with read acess.
			epoll_ctl(epoll_fd, EPOLL_CTL_ADD, *listener.0, &mut event);
		}

		return epoll_fd
	}
}
