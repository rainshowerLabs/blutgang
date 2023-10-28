pub fn execute_method(
	method: Option<&str>,
	params: Option<&str>,
) {
    match method {
        Some("blutgang_quit") => admin_blutgang_quit(),
        // "blutgang_rpc_list" => _,
        // "blutgang_db_stats" => _,
        // "blutgang_print_db_profile_and_drop" => _,
        // "blutgang_cache" => _,
        // "blutgang_force_reorg" => _,
        // "blutgang_force_health" => _,
        _ => println!("\x1b[31mErr:\x1b[0m Invalid admin namespace method"),
    }
}

pub fn admin_blutgang_quit() {
    std::process::exit(0);
}
