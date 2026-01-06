//! Utilitaries functions.

pub fn debug_print_request(request: &httparse::Request, result: &httparse::Status<usize>) {
    println!("\n--- HTTP REQUEST BEGIN ---");
	println!("Method: {:?}", request.method);
    println!("Path: {:?}", request.path);
    println!("Version: {:?}", request.version);
    for header in request.headers.iter() {
        println!("Header: {}: {}", header.name, String::from_utf8_lossy(header.value));
    }
    println!("Request is {}", if result.is_complete() { "complete" } else { "partial" });
    println!("--- HTTP REQUEST END ---\n");
}
