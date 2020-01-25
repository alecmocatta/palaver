#![cfg(unix)]

use nix::{sys::signal, unistd::Pid, *};
use rand::Rng;
use std::{
	mem, process, sync::{
		atomic::{AtomicBool, Ordering}, Arc
	}, thread::{self, sleep}, time::Duration
};

use palaver::{
	file::pipe, process::{fork, ForkResult}
};

fn kills_grandchild() {
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
	let allocator = thread::spawn(move || {
		let mut rng = rand::thread_rng();
		while !done1.load(Ordering::Relaxed) {
			let a = (0..rng.gen_range(0, 1000u16)).collect::<Vec<_>>();
			sleep(rng.gen_range(Duration::new(0, 0), Duration::from_millis(1)));
			drop(a); // std::hint::black_box when it's stable
		}
	});
	let run = move |thread| {
		for i in 0..iterations {
			kills_grandchild();
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
			let child = if let ForkResult::Parent(child) = fork(false).unwrap() {
				child
			} else {
				unistd::setpgid(unistd::Pid::from_raw(0), unistd::Pid::from_raw(0)).unwrap();
				if let ForkResult::Parent(child) = fork(false).unwrap() {
					assert_eq!(unistd::getpid(), unistd::getpgrp());
					mem::forget(child);
				} else {
					assert_ne!(unistd::getpid(), unistd::getpgrp());
				}
				run(3, 10_000);
				process::exit(0);
			};

			sleep(rand::thread_rng().gen_range(Duration::new(0, 0), Duration::from_millis(500)));
			signal::kill(Pid::from_raw(-child.pid.as_raw()), signal::SIGKILL).unwrap();
			child.wait().unwrap();
			mem::forget(child);
		}
	})
}

// Run sequentially
#[test]
fn tests() {
	println!("kills_grandchild");
	kills_grandchild();
	println!("multithreaded");
	multithreaded();
	println!("group_kill");
	group_kill();
	println!("done");
}

fn assert_dead<R>(f: impl FnOnce() -> R) -> R {
	let tmpdir = (0..10)
		.map(|_| rand::thread_rng().sample(rand::distributions::Alphanumeric))
		.collect::<String>();
	std::fs::create_dir(&tmpdir).unwrap();
	std::env::set_current_dir(&tmpdir).unwrap();
	let ret = f();
	std::env::set_current_dir("..").unwrap();
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
	assert_eq!(of, 0, "{}", String::from_utf8_lossy(&out));
	ret
}
