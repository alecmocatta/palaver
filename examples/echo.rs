//! Echo the command line arguments and environment variables
//!
//! This exists for `tests/env.rs`

use palaver::env;

fn main() {
	println!(
		"{}",
		serde_json::to_string(&(env::args_os(), env::vars_os())).unwrap()
	);
}
