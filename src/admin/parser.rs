pub fn entrypoint(request: &str) {
	match request {
		"blutgang_quit" => std::process::exit(0),
		_ => println!("\x1b[31mErr:\x1b[0m Invalid admin namespace method"),
	}
}