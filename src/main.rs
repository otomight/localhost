mod config;
mod utils;
mod global;
mod setup;
mod client;
mod parse_req;
mod router;
mod upload_handler;

use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::time::{Duration, Instant};
use libc::{
	EPOLL_CTL_MOD, EPOLLIN, EPOLLOUT, epoll_ctl, epoll_event, epoll_wait
};

use crate::client::close_client;
use crate::config::Config;
use crate::global::{MAX_EVENTS, RATELIMITER_REQUEST_NUMBER, RATELIMITER_WINDOW};
use crate::setup::ListenerCtx;
use crate::utils::get_error_body;

fn event_loop(
	epoll_fd: RawFd,
	listeners: &HashMap<RawFd, ListenerCtx>,
	events: &mut [epoll_event],
	clients: &mut HashMap<RawFd, client::Client>,
) {
	let mut nb_event = 0;
	let mut last_second = Instant::now();
	loop {
		if last_second.elapsed() >= Duration::from_millis(RATELIMITER_WINDOW) {
			nb_event = 0;
			last_second = Instant::now();
		}

		let nfds: i32 = unsafe {
			// Waiting for readiness events on file descriptors registred with a timeout.
			epoll_wait(epoll_fd, events.as_mut_ptr(), events.len() as i32, 1000)
		};

		// Check if there is any file descriptor ready to be treated.
		for i in 0..nfds as usize {
			let fd = events[i].u64 as RawFd;
			let ev = events[i].events;
			nb_event += 1;

			if nb_event < RATELIMITER_REQUEST_NUMBER {
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
			} else {
				let client = match clients.get_mut(&fd) {
					Some(c) => c,
					None => continue,
				};

				client.write_buf = get_error_body(429, "Too Many Requests", None);

				let mut event = epoll_event {
					events: EPOLLOUT as u32,
					u64: fd as u64,
				};

				unsafe {
					epoll_ctl(epoll_fd, EPOLL_CTL_MOD, fd, &mut event);
					libc::write(
						fd,
						client.write_buf.as_ptr() as *const _,
						client.write_buf.len(),
					)
				};

				close_client(epoll_fd, client.fd, clients);
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
	// println!("{:?}", config);

	let listeners = setup::create_listeners(&config);
	// println!("{:?}", listeners);
	let epoll_fd = setup::setup_epoll(&listeners);

	let mut events = vec![epoll_event { events: 0, u64: 0 }; MAX_EVENTS];
	let mut clients: HashMap<RawFd, client::Client> = HashMap::new();

	event_loop(epoll_fd, &listeners, &mut events, &mut clients);
}

