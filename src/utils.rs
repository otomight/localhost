//! Utilitaries functions.

use crate::client::Client;

pub fn debug_print_request(client: &Client) {
	match std::str::from_utf8(&client.read_buf) {
		Ok(text) => {
			println!("--- HTTP REQUEST BEGIN ---");
			println!("{}", text);
			println!("--- HTTP REQUEST END ---");
		}
		Err(_) => {
			println!("--- HTTP REQUEST (non-UTF8) ---");
			println!("{:?}", client.read_buf);
		}
	}
}
