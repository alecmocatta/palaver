use palaver::proc::*;
#[cfg(unix)]
use std::fs;

fn main() {
	let _ = exe().unwrap();
	#[cfg(unix)]
	{
		// Rust testing framework occasionally gives us 0, 1, 2, 6 ???
		assert_eq!(FdIter::new().unwrap().collect::<Vec<_>>()[..3], [0, 1, 2]);
		for fd in FdIter::new().unwrap().take(3) {
			println!("{:?}", fd);
			let _ = fs::File::open(fd_path(fd).unwrap()).unwrap();
		}
	}
}
