use palaver::env::exe;
#[cfg(unix)]
use palaver::file::FdIter;

fn main() {
	let _ = exe().unwrap();
	#[cfg(unix)]
	{
		// Rust testing framework occasionally gives us 0, 1, 2, 6 ???
		assert_eq!(FdIter::new().unwrap().collect::<Vec<_>>()[..3], [0, 1, 2]);
		for fd in FdIter::new().unwrap().take(3) {
			println!("{:?}", fd);
			// seems to fail on Azure Pipeline's ubuntu-16.04, possibly as it's containerized?
			// let _ = fs::File::open(fd_path(fd).unwrap()).unwrap();
		}
	}
}
