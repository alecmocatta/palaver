#![cfg(unix)]

use nix::*;
use rand::Rng;
use std::{
	process, sync::{
		atomic::{AtomicBool, Ordering}, Arc
	}, thread::{self, sleep}, time::Duration
};

use palaver::{
	file::pipe, process::{fork, ForkResult}
};

#[inline]
fn abort_on_unwind<F: FnOnce() -> T, T>(f: F) -> T {
	replace_with::on_unwind(f, || {
		std::process::abort();
	})
}

#[test]
fn test_kills_grandchild() {
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

	let mut flags =
		fcntl::OFlag::from_bits_truncate(fcntl::fcntl(read, fcntl::FcntlArg::F_GETFL).unwrap());
	flags |= fcntl::OFlag::O_NONBLOCK;
	let err = fcntl::fcntl(read, fcntl::FcntlArg::F_SETFL(flags)).unwrap();
	assert_eq!(err, 0);
	let err = unistd::read(read, &mut [0]);
	assert_eq!(err, Err(nix::Error::Sys(nix::errno::Errno::EAGAIN)));
	flags &= !fcntl::OFlag::O_NONBLOCK;
	let err = fcntl::fcntl(read, fcntl::FcntlArg::F_SETFL(flags)).unwrap();
	assert_eq!(err, 0);

	if rand::random() {
		drop(child);
	} else {
		sys::signal::kill(child.pid, sys::signal::SIGKILL).unwrap();
	}
	let err = unistd::read(read, &mut [0]).unwrap();
	assert_eq!(err, 0);
	unistd::close(read).unwrap();
}

fn run(threads: usize, iterations: usize) {
	let done = Arc::new(AtomicBool::new(false));
	let done1 = done.clone();
	thread::spawn(move || {
		abort_on_unwind(|| {
			let mut rng = rand::thread_rng();
			while !done1.load(Ordering::Relaxed) {
				let a = (0..rng.gen_range(0, 1000u16)).collect::<Vec<_>>();
				sleep(rng.gen_range(Duration::new(0, 0), Duration::from_millis(1)));
				drop(a); // std::hint::black_box when it's stable
			}
		})
	});
	let run = move |thread| {
		for i in 0..iterations {
			test_kills_grandchild();
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
}

#[test]
fn main() {
	abort_on_unwind(|| {
		let pid = unistd::getpid();
		let group = unistd::getpgrp();
		let as_group_leader = pid == group;
		run(10, 10_000);

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
