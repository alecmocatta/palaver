use super::*;
use ext::ToHex;
#[cfg(unix)]
use nix::{errno, fcntl, sys::stat, unistd};
use proc_self::fd_path;
#[cfg(unix)]
use std::{
	ffi::{CStr, CString}, os::unix::io::AsRawFd
};
use std::{
	fs, io::{self, Read, Write}, mem, path
};

/// Maps file descriptors [(from,to)]
pub fn move_fds(fds: &mut [(Fd, Fd)]) {
	loop {
		#[allow(clippy::never_loop)]
		let i = 'a: loop {
			for (i, &(from, to)) in fds.iter().enumerate() {
				if from == to {
					continue;
				}
				if fds.iter().position(|&(from, _)| from == to).is_none() {
					break 'a i;
				}
			}
			for &mut (from, to) in fds {
				assert_eq!(from, to); // this assertion checks we aren't looping eternally due to a ring (todo: use self::dup for temp fd)
			}
			return;
		};
		let (from, to) = fds[i];
		let flags = fcntl::FdFlag::from_bits(fcntl::fcntl(from, fcntl::FcntlArg::F_GETFD).unwrap())
			.unwrap();
		dup_to(
			from,
			to,
			if flags.contains(fcntl::FdFlag::FD_CLOEXEC) {
				fcntl::OFlag::O_CLOEXEC
			} else {
				fcntl::OFlag::empty()
			},
		)
		.unwrap();
		let _ = fcntl::fcntl(to, fcntl::FcntlArg::F_SETFD(flags)).unwrap();
		unistd::close(from).unwrap();
		fds[i].0 = to;
	}
}

/// Makes a file descriptor read-only, which seems neccessary on some platforms to pass to fexecve and is good practise anyway
pub fn seal(fd: Fd) {
	let fd2 = fcntl::open(
		&fd_path(fd).unwrap(),
		fcntl::OFlag::O_RDONLY,
		stat::Mode::empty(),
	)
	.unwrap();
	let flags =
		fcntl::FdFlag::from_bits(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap()).unwrap();
	unistd::close(fd).unwrap();
	dup_to(
		fd2,
		fd,
		if flags.contains(fcntl::FdFlag::FD_CLOEXEC) {
			fcntl::OFlag::O_CLOEXEC
		} else {
			fcntl::OFlag::empty()
		},
	)
	.unwrap();
	unistd::close(fd2).unwrap();
}

/// Like dup except O_CLOEXEC can be passed atomically
pub fn dup(oldfd: Fd, flags: fcntl::OFlag) -> Result<Fd, nix::Error> {
	fcntl::fcntl(
		oldfd,
		if flags.contains(fcntl::OFlag::O_CLOEXEC) {
			fcntl::FcntlArg::F_DUPFD_CLOEXEC(oldfd)
		} else {
			fcntl::FcntlArg::F_DUPFD(oldfd)
		},
	)
	.map(|newfd| {
		assert_ne!(oldfd, newfd);
		newfd
	})
}
/// Like dup2/3; automatically retries on EBUSY on Linux
pub fn dup_to(oldfd: Fd, newfd: Fd, flags: fcntl::OFlag) -> Result<(), nix::Error> {
	assert_ne!(oldfd, newfd);
	#[cfg_attr(
		not(any(target_os = "android", target_os = "linux")),
		allow(clippy::never_loop)
	)]
	loop {
		match unistd::dup3(oldfd, newfd, flags) {
			#[cfg(any(target_os = "android", target_os = "linux"))]
			Err(nix::Error::Sys(errno::Errno::EBUSY)) => continue, // only occurs on Linux
			a => break a,
		}
	}
	.map(|fd| assert_eq!(fd, newfd))
}

/// Like pipe2; not atomic on platforms that lack it
pub fn pipe(flags: fcntl::OFlag) -> Result<(Fd, Fd), nix::Error> {
	#[cfg(any(
		target_os = "android",
		target_os = "dragonfly",
		target_os = "emscripten",
		target_os = "freebsd",
		target_os = "linux",
		target_os = "netbsd",
		target_os = "openbsd"
	))]
	{
		unistd::pipe2(flags)
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "dragonfly",
		target_os = "emscripten",
		target_os = "freebsd",
		target_os = "linux",
		target_os = "netbsd",
		target_os = "openbsd"
	)))]
	{
		unistd::pipe().map(|(read, write)| {
			fn apply(fd: Fd, new_flags: fcntl::OFlag) {
				let mut flags =
					fcntl::OFlag::from_bits(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFL).unwrap())
						.unwrap();
				flags |= new_flags & !fcntl::OFlag::O_CLOEXEC;
				let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFL(flags)).unwrap();
				let mut flags =
					fcntl::FdFlag::from_bits(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap())
						.unwrap();
				flags.set(
					fcntl::FdFlag::FD_CLOEXEC,
					new_flags.contains(fcntl::OFlag::O_CLOEXEC),
				);
				let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFD(flags)).unwrap();
			}
			apply(read, flags);
			apply(write, flags);
			(read, write)
		})
	}
}

/// Falls back to shm_open, falls back to creating+unlinking /tmp/{random_filename}
pub fn memfd_create(name: &CStr, cloexec: bool) -> Result<Fd, nix::Error> {
	let ret = {
		#[cfg(any(target_os = "android", target_os = "linux"))]
		{
			use nix::sys::memfd;
			let mut flags = memfd::MemFdCreateFlag::empty();
			flags.set(memfd::MemFdCreateFlag::MFD_CLOEXEC, cloexec);
			memfd::memfd_create(name, flags)
			// Err(nix::Error::Sys(errno::Errno::ENOSYS)) => None,
		}
		#[cfg(target_os = "freebsd")]
		{
			let _ = name;
			let flags = if cloexec { fcntl::OFlag::O_RDWR | fcntl::OFlag::O_CLOEXEC } else { fcntl::OFlag::O_RDWR };
			errno::Errno::result(unsafe {
				libc::shm_open(libc::SHM_ANON, flags.bits(), stat::Mode::S_IRWXU.bits())
			})
		}
		#[cfg(not(any(target_os = "android", target_os = "linux", target_os = "freebsd")))]
		{
			let _ = name;
			Err(nix::Error::Sys(errno::Errno::ENOSYS))
		}
	};
	#[cfg(not(any(target_os = "ios", target_os = "macos")))] // can't read/write on mac
	let ret = ret.or_else(|_e| {
		use nix::sys::mman;
		let mut random: [u8; 16] = unsafe { mem::uninitialized() }; // ENAMETOOLONG on mac for 16
															  // thread_rng uses getrandom(2) on >=3.17 (same as memfd_create), permanently opens /dev/urandom on fail, which messes our fd numbers. TODO: less assumptive about fd numbers..
		let rand = fs::File::open("/dev/urandom").expect("Couldn't open /dev/urandom");
		(&rand).read_exact(&mut random).unwrap();
		drop(rand);
		let name = path::PathBuf::from(format!("/{}", random.to_hex()));
		mman::shm_open(
			&name,
			fcntl::OFlag::O_RDWR | fcntl::OFlag::O_CREAT | fcntl::OFlag::O_EXCL,
			stat::Mode::S_IRWXU,
		)
		.map(|fd| {
			if !cloexec {
				let mut flags_ =
					fcntl::FdFlag::from_bits(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap())
						.unwrap();
				flags_.remove(fcntl::FdFlag::FD_CLOEXEC);
				let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFD(flags_)).unwrap();
			}
			mman::shm_unlink(&name).unwrap();
			fd
		})
	});
	ret.or_else(|_e| {
		let mut random: [u8; 16] = unsafe { mem::uninitialized() };
		// thread_rng uses getrandom(2) on >=3.17 (same as memfd_create), permanently opens /dev/urandom on fail, which messes our fd numbers. TODO: less assumptive about fd numbers..
		let rand = fs::File::open("/dev/urandom").expect("Couldn't open /dev/urandom");
		(&rand).read_exact(&mut random).unwrap();
		drop(rand);
		let name = path::PathBuf::from(format!("/tmp/{}_XXXXXX", random.to_hex()));
		unistd::mkstemp(&name).map(|(fd, path)| {
			unistd::unlink(path.as_path()).unwrap();
			stat::fchmod(fd, stat::Mode::S_IRWXU).unwrap();
			if cloexec {
				let mut flags_ =
					fcntl::FdFlag::from_bits(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap())
						.unwrap();
				flags_.insert(fcntl::FdFlag::FD_CLOEXEC);
				let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFD(flags_)).unwrap();
			}
			fd
		})
	})
}

/// Falls back to execve("/proc/self/fd/{fd}",...), falls back to execve("/tmp/{randomfilename}")
pub fn fexecve(fd: Fd, arg: &[CString], env: &[CString]) -> Result<void::Void, nix::Error> {
	#[cfg(any(
		target_os = "android",
		target_os = "freebsd",
		target_os = "linux",
		target_os = "netbsd",
		target_os = "openbsd"
	))]
	{
		unistd::fexecve(fd, arg, env)
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "freebsd",
		target_os = "linux",
		target_os = "netbsd",
		target_os = "openbsd"
	)))]
	{
		use std::{
			ffi::OsString, os::unix::{ffi::OsStringExt, io::FromRawFd}, process
		};
		unistd::execve(
			&CString::new(<OsString as OsStringExt>::into_vec(
				fd_path(fd).unwrap().into(),
			))
			.unwrap(),
			arg,
			env,
		)
		.or_else(|_e| {
			let mut random: [u8; 16] = unsafe { mem::uninitialized() };
			// thread_rng uses getrandom(2) on >=3.17 (same as memfd_create), permanently opens /dev/urandom on fail, which messes our fd numbers. TODO: less assumptive about fd numbers..
			let rand = fs::File::open("/dev/urandom").expect("Couldn't open /dev/urandom");
			(&rand).read_exact(&mut random).unwrap();
			drop(rand);
			let name = path::PathBuf::from(format!("/tmp/{}_XXXXXX", random.to_hex()));
			let (to, to_path) = unistd::mkstemp(&name)
				.map(|(fd, path)| {
					// unistd::unlink(path.as_path()).unwrap();
					stat::fchmod(fd, stat::Mode::S_IRWXU).unwrap();
					if true {
						// cloexec
						let mut flags_ = fcntl::FdFlag::from_bits(
							fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap(),
						)
						.unwrap();
						flags_.insert(fcntl::FdFlag::FD_CLOEXEC);
						let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFD(flags_)).unwrap();
					}
					(fd, path)
				})
				.unwrap();
			let from = unsafe { fs::File::from_raw_fd(fd) };
			let to = unsafe { fs::File::from_raw_fd(to) };
			let x = unistd::lseek(from.as_raw_fd(), 0, unistd::Whence::SeekSet).unwrap();
			assert_eq!(x, 0);
			let _ = io::copy(&mut &from, &mut &to).unwrap(); // copyfile?
			assert_eq!(from.metadata().unwrap().len(), to.metadata().unwrap().len());
			let (read, write) = pipe(fcntl::OFlag::O_CLOEXEC).unwrap();
			if let unistd::ForkResult::Parent { .. } = unistd::fork().expect("Fork failed") {
				unistd::close(read).unwrap();
				unistd::execve(
					&CString::new(<OsString as OsStringExt>::into_vec(to_path.into())).unwrap(),
					arg,
					env,
				)
				.map_err(|e| {
					let _ = unistd::write(write, &[0]).unwrap();
					unistd::close(write).unwrap();
					e
				})
			} else {
				unistd::close(write).unwrap();
				match unistd::read(read, &mut [0, 0]) {
					Ok(1) => process::exit(0),
					Ok(0) => {
						// constellation currently relies upon current_exe() on mac not having been deleted
						// unistd::unlink(to_path.as_path()).unwrap();
						process::exit(0)
					}
					e => panic!("{:?}", e),
				}
			}
		})
	}
}

/// Loops io::copy till len elapsed or error
pub fn copy<R: ?Sized, W: ?Sized>(reader: &mut R, writer: &mut W, len: u64) -> io::Result<()>
where
	R: Read,
	W: Write,
{
	let mut offset = 0;
	while offset != len {
		offset += io::copy(&mut reader.take(len - offset), writer)?;
	}
	Ok(())
}
/// Loops sendfile till len elapsed or error
pub fn copy_sendfile<O: AsRawFd, I: AsRawFd>(in_: &I, out: &O, len: u64) -> Result<(), nix::Error> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		use nix::sys::sendfile;
		let mut offset = 0;
		while offset != len {
			offset += sendfile::sendfile(
				out.as_raw_fd(),
				in_.as_raw_fd(),
				None,
				(len - offset) as usize,
			)? as u64;
		}
		Ok(())
	}
	#[cfg(any(target_os = "ios", target_os = "macos"))]
	{
		use nix::sys::sendfile;
		let mut offset = 0;
		while offset != len {
			let (result, n) = sendfile::sendfile(
				in_.as_raw_fd(),
				out.as_raw_fd(),
				0,
				Some((len - offset) as i64),
				None,
				None,
			);
			result?;
			assert!(0 < n && n as u64 <= len - offset);
			offset += n as u64;
		}
		Ok(())
	}
}
/// Loops splice till len elapsed or error
#[cfg(any(target_os = "android", target_os = "linux"))]
pub fn copy_splice<O: AsRawFd, I: AsRawFd>(in_: &I, out: &O, len: u64) -> Result<(), nix::Error> {
	let mut offset = 0;
	while offset != len {
		offset += fcntl::splice(
			in_.as_raw_fd(),
			None,
			out.as_raw_fd(),
			None,
			(len - offset) as usize,
			fcntl::SpliceFFlags::empty(),
		)? as u64;
	}
	Ok(())
}
