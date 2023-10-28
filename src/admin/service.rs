pub fn parse_method(method: &str) {
	match method {
		"blutgang_quit" => std::process::exit(0),
		"blutgang_rpc_list" => _,
		"blutgang_db_stats" => _,
		"blutgang_print_db_profile_and_drop" => _,
		"blutgang_cache" => _,
		"blutgang_force_reorg" => _,
		"blutgang_force_health" => _,
		_ => println!("\x1b[31mErr:\x1b[0m Invalid admin namespace method"),
	}
}
