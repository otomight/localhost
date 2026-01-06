mod config;
mod utils;
mod global;
mod setup;
mod client;

use std::collections::HashMap;
use std::os::unix::io::RawFd;
use libc::{
	epoll_event, epoll_wait, EPOLLIN, EPOLLOUT
};

use crate::config::Config;
use crate::global::MAX_EVENTS;
use crate::setup::ListenerCtx;

fn event_loop(
	epoll_fd: RawFd,
	listeners: &HashMap<RawFd, ListenerCtx>,
	events: &mut [epoll_event],
	clients: &mut HashMap<RawFd, client::Client>,
) {
	loop {
		let nfds = unsafe {
			// Waiting for readiness events on file descriptors registred with a timeout.
			epoll_wait(epoll_fd, events.as_mut_ptr(), events.len() as i32, 1000)
		};
		// Check if there is any file descriptor ready to be treated.
		if nfds < 0 {
			continue;
		}

		for i in 0..nfds as usize {
			let fd = events[i].u64 as RawFd;
			let ev = events[i].events;

			// Check if the event is related to a listener.
			if let Some(listener_ctx) = listeners.get(&fd) {
				client::handle_listener_event(epoll_fd, listener_ctx, fd, clients);
			// It's not a listener then its a client.
			} else {
				// If the client has read access.
				if ev & EPOLLIN as u32 != 0 {
					client::handle_client_read(epoll_fd, fd, clients, listeners);
				// If the client has write access.
				} else if ev & EPOLLOUT as u32 != 0 {
					client::handle_client_write(epoll_fd, fd, clients);
				}
			}
		}
	}
}

fn main() {
	let config = match Config::from_file(global::CONFIG_PATH) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("{}", e);
			return;
		}
	};

	let listeners = setup::create_listeners(&config);
	let epoll_fd = setup::setup_epoll(&listeners);

	let mut events = vec![epoll_event { events: 0, u64: 0 }; MAX_EVENTS];
	let mut clients: HashMap<RawFd, client::Client> = HashMap::new();

	event_loop(epoll_fd, &listeners, &mut events, &mut clients);
}

