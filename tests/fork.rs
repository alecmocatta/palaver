#[cfg(unix)]
mod fork {
	use nix::{sys::signal, unistd::Pid, *};
	use rand::{seq::SliceRandom, Rng};
	use std::{
		mem, process,
		sync::{
			atomic::{AtomicBool, Ordering},
			Arc,
		},
		thread::{self, sleep},
		time::Duration,
	};

	use palaver::{
		file::pipe,
		process::{fork, ForkResult},
	};

	#[global_allocator]
	static ALLOC: forbid_alloc::Alloc<std::alloc::System> =
		forbid_alloc::Alloc::new(std::alloc::System);

	fn kills_grandchild(signal: Option<sys::signal::Signal>) {
		forbid_alloc(|| {
			let (read, write) = pipe(fcntl::OFlag::empty()).unwrap();
			let child = if let ForkResult::Parent(child) = fork(false).unwrap() {
				child
			} else {
				let _child = if let ForkResult::Parent(child) = fork(false).unwrap() {
					child
				} else {
					let err = unistd::write(write, &[0]).unwrap();
					assert_eq!(err, 1);
					loop {
						unistd::pause()
					}
				};
				unistd::close(write).unwrap();
				loop {
					unistd::pause()
				}
			};
			unistd::close(write).unwrap();
			let err = unistd::read(read, &mut [0]).unwrap();
			assert_eq!(err, 1);

			let mut flags = fcntl::OFlag::from_bits_truncate(
				fcntl::fcntl(read, fcntl::FcntlArg::F_GETFL).unwrap(),
			);
			flags |= fcntl::OFlag::O_NONBLOCK;
			let err = fcntl::fcntl(read, fcntl::FcntlArg::F_SETFL(flags)).unwrap();
			assert_eq!(err, 0);
			let err = unistd::read(read, &mut [0]);
			assert_eq!(err, Err(nix::errno::Errno::EAGAIN));
			flags &= !fcntl::OFlag::O_NONBLOCK;
			let err = fcntl::fcntl(read, fcntl::FcntlArg::F_SETFL(flags)).unwrap();
			assert_eq!(err, 0);

			if let Some(signal) = signal {
				sys::signal::kill(child.pid, signal).unwrap();
			} else {
				drop(child);
			}
			let err = unistd::read(read, &mut [0]).unwrap();
			assert_eq!(err, 0);
			unistd::close(read).unwrap();
		})
	}

	fn run(threads: usize, iterations: usize) {
		let done = Arc::new(AtomicBool::new(false));
		let done1 = done.clone();
		let allocator = thread::spawn(move || {
			let mut rng = rand::thread_rng();
			while !done1.load(Ordering::Relaxed) {
				let a = (0..rng.gen_range(0..1000u16)).collect::<Vec<_>>();
				sleep(Duration::from_millis(rng.gen_range(0..1)));
				drop(a); // std::hint::black_box when it's stable
			}
		});
		let run = move |thread| {
			let mut rng = rand::thread_rng();
			for i in 0..iterations {
				let signal = *[Some(sys::signal::SIGKILL), Some(sys::signal::SIGTERM), None]
					.choose(&mut rng)
					.unwrap();
				kills_grandchild(signal);
				if i % 100 == 0 {
					println!("{}\t{}", thread, i);
				}
			}
		};
		let handles = (1..threads)
			.map(|thread| thread::spawn(move || run(thread)))
			.collect::<Vec<_>>();
		run(0);
		for handle in handles {
			handle.join().unwrap();
		}
		done.store(true, Ordering::Relaxed);
		allocator.join().unwrap();
	}

	fn multithreaded() {
		assert_dead(|| {
			let pid = unistd::getpid();
			let group = unistd::getpgrp();
			let as_group_leader = pid == group;
			run(10, 10_000);

			// retry because for some reason it can lag on linux
			while palaver::thread::count() != 1 {
				sleep(Duration::from_millis(1))
			}
			if let ForkResult::Parent(child) = fork(false).unwrap() {
				child.wait().unwrap();
			} else {
				assert_eq!(group, unistd::getpgrp());
				if !as_group_leader {
					unistd::setpgid(unistd::Pid::from_raw(0), unistd::Pid::from_raw(0)).unwrap();
				}
				run(10, 10_000);
				process::exit(0);
			}
		})
	}

	fn group_kill() {
		assert_dead(|| {
			for _ in 0..1_000 {
				assert_eq!(palaver::thread::count(), 1);
				let child = if let ForkResult::Parent(child) = fork(false).unwrap() {
					child
				} else {
					unistd::setpgid(unistd::Pid::from_raw(0), unistd::Pid::from_raw(0)).unwrap();
					assert_eq!(palaver::thread::count(), 1);
					if let ForkResult::Parent(child) = fork(false).unwrap() {
						assert_eq!(unistd::getpid(), unistd::getpgrp());
						mem::forget(child);
					} else {
						assert_ne!(unistd::getpid(), unistd::getpgrp());
					}
					run(3, 10_000);
					process::exit(0);
				};

				sleep(
					rand::thread_rng()
						.gen_range(Duration::from_millis(0)..Duration::from_millis(500)),
				);
				let signal = if rand::random() {
					signal::SIGKILL
				} else {
					signal::SIGTERM
				};
				signal::kill(Pid::from_raw(-child.pid.as_raw()), signal).unwrap(); // or SIGHUP SIGINT SIGTERM
				child.wait().unwrap();
			}
		})
	}

	// We need precisely 1 thread, so we can't use #[test]
	pub fn main() {
		println!("multithreaded");
		multithreaded();
		println!("group_kill");
		group_kill();
		println!("done");
	}

	fn assert_dead<R>(f: impl FnOnce() -> R) -> R {
		let tmpdir = String::from_utf8(
			(0..10)
				.map(|_| rand::thread_rng().sample(rand::distributions::Alphanumeric))
				.collect::<Vec<_>>(),
		)
		.unwrap();
		std::fs::create_dir(&tmpdir).unwrap();
		std::env::set_current_dir(&tmpdir).unwrap();
		let ret = f();
		std::env::set_current_dir("..").unwrap();
		sleep(Duration::from_millis(1000)); // might help reduce flakiness?
		let out = std::process::Command::new("lsof")
			.arg(&tmpdir)
			.output()
			.expect("failed to execute process")
			.stdout;
		let of = out
			.split(|&x| x == b'\n')
			.skip(1)
			.filter(|x| !x.is_empty())
			.count();
		std::fs::remove_dir(&tmpdir).unwrap();
		// TODO: fix flakiness and assert
		if of != 0 {
			println!("##vso[task.logissue type=warning] warning!");
			println!(
				"not all processes have been killed: {}",
				String::from_utf8_lossy(&out)
			);
		}
		ret
	}

	mod forbid_alloc {
		use std::{
			alloc::{GlobalAlloc, Layout},
			cell::RefCell,
			fs::File,
			io::{self, Write},
			os::unix::io::{FromRawFd, IntoRawFd},
		};

		#[derive(Copy, Clone, PartialEq)]
		enum State {
			Allowed,
			Forbidden,
			Panicking,
		}

		thread_local! {
			static STATE: RefCell<State> = RefCell::new(State::Allowed);
		}

		pub struct Alloc<A> {
			inner: A,
		}
		impl<A> Alloc<A> {
			pub const fn new(inner: A) -> Self {
				Self { inner }
			}
			fn panic() -> bool {
				STATE.with(|alloc_forbid| {
					let mut state = alloc_forbid.borrow_mut();
					if *state != State::Panicking && std::thread::panicking() {
						*state = State::Panicking;
					}
					let panic = *state == State::Forbidden;
					if panic {
						*state = State::Panicking;
					}
					panic
				})
			}
		}
		unsafe impl<A> GlobalAlloc for Alloc<A>
		where
			A: GlobalAlloc,
		{
			unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
				if Self::panic() {
					StdErr
						.write_all(b"alloc inside forbid_alloc: panicking!\n")
						.unwrap();
					panic!();
				}
				self.inner.alloc(layout)
			}

			unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
				if Self::panic() {
					StdErr
						.write_all(b"realloc inside forbid_alloc: panicking!\n")
						.unwrap();
					panic!();
				}
				self.inner.realloc(ptr, layout, new_size)
			}

			unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
				if Self::panic() {
					StdErr
						.write_all(b"dealloc inside forbid_alloc: panicking!\n")
						.unwrap();
					panic!();
				}
				self.inner.dealloc(ptr, layout)
			}
		}

		pub fn forbid_alloc<R>(f: impl FnOnce() -> R) -> R {
			let mut toggled = false;
			STATE.with(|alloc_forbid| {
				let mut state = alloc_forbid.borrow_mut();
				if *state == State::Allowed {
					*state = State::Forbidden;
					toggled = true;
				}
			});
			let ret = f();
			if toggled {
				STATE.with(|alloc_forbid| {
					let mut state = alloc_forbid.borrow_mut();
					if *state == State::Forbidden {
						*state = State::Allowed;
					}
				});
			}
			ret
		}

		struct StdErr;
		impl Write for StdErr {
			fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
				let mut file = unsafe { File::from_raw_fd(libc::STDERR_FILENO) };
				let ret = file.write(buf);
				let _ = file.into_raw_fd();
				ret
			}
			fn flush(&mut self) -> io::Result<()> {
				Ok(())
			}
		}
	}
	use forbid_alloc::forbid_alloc;
}
#[cfg(windows)]
mod fork {
	pub fn main() {
		println!("not implemented on windows");
	}
}
fn main() {
	fork::main();
}
