## Config Example
```conf
# Virtual host
server {
	# Base config
	host 0.0.0.0 					# IP adress where the server will listen
	ports 8080 8081 				# Ports that will be opened to listen on
	server_name localhost 			# Domain name
	client_max_body_size 1048576 	# Max request size

	# Errors
	error_page 404 errors/404.html
	error_page 500 errors/500.html

	# Routing
	route / { 						# Index route with methods
		methods GET POST DELETE
		page index.html
	}
	route /program { 				# Program route with CGI in python
		methods GET
		cgi_ext .py
		cgi_path /usr/bin/python3
	}
}
```
> In `route { ... }`, only methods is mandatory, all other options (`redirect`,`page`,`cgi_ext`, `cgi_path`) are optional, `cgi_[...]` are always together.