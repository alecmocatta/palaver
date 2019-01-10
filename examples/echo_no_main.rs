//! Echo the command line arguments and environment variables
//!
//! This exists for `tests/env.rs`

#![no_main]

use palaver::env;

#[no_mangle]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
	println!(
		"{}",
		serde_json::to_string(&(env::args_os(), env::vars_os())).unwrap()
	);
	0
}
