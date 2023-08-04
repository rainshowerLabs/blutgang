// All as floats so we have an easier time getting averages, stats and terminology copied from flood
struct Status {
	is_erroring: bool
	latency: f64,
	throughput: f64,
}

#[derive(Debug)]
struct Rpc {
	url: String,
	status: Status,
}
