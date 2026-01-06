//! Parse the server config file and return a config object.

use std::collections::HashMap;
use std::fs;
use std::io;


#[derive(Debug, Clone)]
pub struct Config {
	pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
	pub host: String,
	pub ports: Vec<u16>,
	pub server_name: Option<String>,
	pub error_pages: HashMap<u16, String>,
	pub client_max_body_size: usize,
	pub routes: Vec<Route>,
}

#[derive(Debug, Clone)]
pub struct Route {
	pub path: String,
	pub methods: Vec<String>,
	pub redirect: Option<String>,
	pub index: Option<String>,
	pub cgi_extension: Option<String>,
	pub cgi_path: Option<String>,
}


impl Config {
	pub fn from_file(path: &str) -> io::Result<Self> {
		let content = fs::read_to_string(path)?;

		let mut lines = content
			.lines()
			.map(|l| l.trim())
			.filter(|l| !l.is_empty())
			.peekable();

		let mut servers = Vec::new();

		while let Some(line) = lines.next() {
			if line == "server {" {
				servers.push(parse_server(&mut lines)?);
			} else {
				return Err(io::Error::new(
					io::ErrorKind::InvalidData,
					"Expected `server {`",
				));
			}
		}

		Ok(Config { servers })
	}
}

fn parse_server<'a, I>(lines: &mut std::iter::Peekable<I>) -> io::Result<ServerConfig>
where
	I: Iterator<Item = &'a str>,
{
	let mut host = "0.0.0.0".to_string();
	let mut ports = Vec::new();
	let mut server_name = None;
	let mut error_pages = HashMap::new();
	let mut client_max_body_size = 1_048_576;
	let mut routes = Vec::new();

	while let Some(line) = lines.next() {
		if line == "}" {
			break;
		}

		let parts: Vec<&str> = line.split_whitespace().collect();
		if parts.is_empty() {
			continue;
		}

		match parts[0] {
			"host" => host = parts[1].to_string(),
			"ports" => {
				ports = parts[1..].iter().map(|s| s.parse::<u16>().unwrap()).collect();
			}
			"server_name" => server_name = Some(parts[1].to_string()),
			"client_max_body_size" => {
				client_max_body_size = parts[1].parse().unwrap()
			}
			"error_page" => {
				let code: u16 = parts[1].parse().unwrap();
				error_pages.insert(code, parts[2].to_string());
			}
			"route" if parts[2] == "{" => {
				routes.push(parse_route(parts[1], lines)?);
			}
			_ => {}
		}
	}

	Ok(ServerConfig {
		host,
		ports,
		server_name,
		error_pages,
		client_max_body_size,
		routes,
	})
}

fn parse_route<'a, I>(path: &str, lines: &mut std::iter::Peekable<I>) -> io::Result<Route>
where
	I: Iterator<Item = &'a str>,
{
	let mut methods = Vec::new();
	let mut redirect = None;
	let mut index = None;
	let mut cgi_extension = None;
	let mut cgi_path = None;

	while let Some(line) = lines.next() {
		if line == "}" {
			break;
		}

		let parts: Vec<&str> = line.split_whitespace().collect();
		if parts.is_empty() {
			continue;
		}

		match parts[0] {
			"methods" => {
				methods = parts[1..].iter().map(|s| s.to_string()).collect();
			}
			"redirect" => redirect = Some(parts[1].to_string()),
			"page" => index = Some(parts[1].to_string()),
			"cgi_ext" => cgi_extension = Some(parts[1].to_string()),
			"cgi_path" => cgi_path = Some(parts[1].to_string()),
			_ => {}
		}
	}

	Ok(Route {
		path: path.to_string(),
		methods,
		redirect,
		index,
		cgi_extension,
		cgi_path,
	})
}

