//! File and file descriptor-related functionality

use super::*;
#[cfg(unix)]
use ext::ToHex;
#[cfg(unix)]
use nix::{errno, fcntl, sys::stat, unistd};
#[cfg(unix)]
use std::{
	ffi::{CStr, CString, OsString}, fs, mem, os::unix::ffi::OsStringExt, os::unix::io::AsRawFd
};
use std::{
	fmt, io::{self, Read, Write}, path
};
#[cfg(any(
	target_os = "linux",
	target_os = "android",
	target_os = "macos",
	target_os = "ios",
	target_os = "freebsd"
))]
use try_from::TryInto;

/// Maps file descriptors [(from,to)]
#[cfg(unix)]
pub fn move_fds(fds: &mut [(Fd, Fd)], flags: Option<fcntl::FdFlag>, allow_nonexistent: bool) {
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
				assert_eq!(from, to); // this assertion checks we aren't looping eternally due to a ring; TODO: use self::dup for temp fd
			}
			return;
		};
		let (from, to) = fds[i];
		move_fd(from, to, flags, allow_nonexistent).unwrap();
		fds[i].0 = to;
	}
}

/// Makes a file descriptor read-only, which seems neccessary on some platforms to pass to fexecve and is good practise anyway.
#[cfg(unix)]
pub fn seal_fd(fd: Fd) {
	let fd2 = fcntl::open(
		&fd_path(fd).unwrap(),
		fcntl::OFlag::O_RDONLY,
		stat::Mode::empty(),
	)
	.unwrap();
	let fd_flags =
		fcntl::FdFlag::from_bits(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap()).unwrap();
	let fl_flags = fcntl::OFlag::from_bits(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFL).unwrap())
		.unwrap()
		& !(fcntl::OFlag::O_WRONLY | fcntl::OFlag::O_RDWR)
		| fcntl::OFlag::O_RDONLY;
	unistd::close(fd).unwrap();
	let err = fcntl::fcntl(fd2, fcntl::FcntlArg::F_SETFL(fl_flags)).unwrap();
	assert_eq!(err, 0);
	move_fd(fd2, fd, Some(fd_flags), false).unwrap();
}

/// Duplicate a file descriptor. Flags are passed atomically. `flags` being `None` copies the flags from `oldfd`.
#[cfg(unix)]
pub fn dup_fd(oldfd: Fd, flags: Option<fcntl::FdFlag>) -> Result<Fd, nix::Error> {
	let flags = flags.unwrap_or_else(|| {
		fcntl::FdFlag::from_bits(fcntl::fcntl(oldfd, fcntl::FcntlArg::F_GETFD).unwrap()).unwrap()
	});
	fcntl::fcntl(
		oldfd,
		if flags.contains(fcntl::FdFlag::FD_CLOEXEC) {
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

/// Move a file descriptor. Flags are passed atomically. `flags` being `None` copies the flags from `oldfd`. Panics if `newfd` doesn't exist and `allow_nonexistent` isn't set; this can help debug the race of another thread creating `newfd` and having it deleted from under it by us.
#[cfg(unix)]
pub fn move_fd(
	oldfd: Fd, newfd: Fd, flags: Option<fcntl::FdFlag>, allow_nonexistent: bool,
) -> Result<(), nix::Error> {
	copy_fd(oldfd, newfd, flags, allow_nonexistent).and_then(|()| unistd::close(oldfd))
}

/// Copy a file descriptor. Flags are passed atomically. `flags` being `None` copies the flags from `oldfd`. Panics if `newfd` doesn't exist and `allow_nonexistent` isn't set; this can help debug the race of another thread creating `newfd` and having it deleted from under it by us.
#[cfg(unix)]
pub fn copy_fd(
	oldfd: Fd, newfd: Fd, flags: Option<fcntl::FdFlag>, allow_nonexistent: bool,
) -> Result<(), nix::Error> {
	if !allow_nonexistent {
		let _ = fcntl::fcntl(newfd, fcntl::FcntlArg::F_GETFD).unwrap();
	}
	if oldfd == newfd {
		return Err(nix::Error::Sys(errno::Errno::EINVAL));
	}
	let flags = flags.unwrap_or_else(|| {
		fcntl::FdFlag::from_bits(fcntl::fcntl(oldfd, fcntl::FcntlArg::F_GETFD).unwrap()).unwrap()
	});
	let flags = if flags.contains(fcntl::FdFlag::FD_CLOEXEC) {
		fcntl::OFlag::O_CLOEXEC
	} else {
		fcntl::OFlag::empty()
	};
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
#[cfg(unix)]
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
				let err = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFL(flags)).unwrap();
				assert_eq!(err, 0);
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
#[cfg(unix)]
pub fn memfd_create(name: &CStr, cloexec: bool) -> Result<Fd, nix::Error> {
	let ret = {
		#[cfg(any(target_os = "android", target_os = "linux"))]
		{
			use nix::sys::memfd;
			let mut flags = memfd::MemFdCreateFlag::empty();
			flags.set(memfd::MemFdCreateFlag::MFD_CLOEXEC, cloexec);
			memfd::memfd_create(name, flags)
		}
		#[cfg(target_os = "freebsd")]
		{
			let _ = name;
			let flags = if cloexec {
				fcntl::OFlag::O_RDWR | fcntl::OFlag::O_CLOEXEC
			} else {
				fcntl::OFlag::O_RDWR
			};
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
	#[cfg(all(unix, not(any(target_os = "ios", target_os = "macos"))))] // can't read/write on mac
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
	#[cfg(unix)]
	{
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
					let mut flags_ = fcntl::FdFlag::from_bits(
						fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap(),
					)
					.unwrap();
					flags_.insert(fcntl::FdFlag::FD_CLOEXEC);
					let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFD(flags_)).unwrap();
				}
				fd
			})
		})
	}
	#[cfg(windows)]
	{
		ret.or_else(|_e| {
			unimplemented!()
			// Ok(unsafe { libc::tmpfile() })
		})
	}
}

/// Falls back to execve("/proc/self/fd/{fd}",...), falls back to execve("/tmp/{randomfilename}")
#[cfg(unix)]
pub fn fexecve(fd: Fd, arg: &[CString], env: &[CString]) -> Result<void::Void, nix::Error> {
	#[cfg(any(target_os = "android", target_os = "freebsd", target_os = "linux"))]
	{
		unistd::fexecve(fd, arg, env)
	}
	#[cfg(all(
		unix,
		not(any(target_os = "android", target_os = "freebsd", target_os = "linux"))
	))]
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
					&CString::new(<OsString as OsStringExt>::into_vec(to_path.clone().into()))
						.unwrap(),
					arg,
					env,
				)
				.map_err(|e| {
					let _ = unistd::write(write, &[0]).unwrap();
					unistd::close(write).unwrap();
					unistd::unlink(to_path.as_path()).unwrap();
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
	#[cfg(windows)]
	{
		Err(unimplemented!())
	}
}

/// Loops `io::copy` till len elapsed or error
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

/// Loops `sendfile` till len elapsed or error
#[cfg(unix)]
pub fn copy_sendfile<O: AsRawFd, I: AsRawFd>(in_: &I, out: &O, len: u64) -> Result<(), nix::Error> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		use nix::sys::sendfile;
		let mut offset: u64 = 0;
		while offset != len {
			let n = sendfile::sendfile(
				out.as_raw_fd(),
				in_.as_raw_fd(),
				None,
				(len - offset).try_into().unwrap(),
			)?;
			let n: u64 = n.try_into().unwrap();
			assert!(n <= len - offset);
			offset += n;
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
				Some((len - offset).try_into().unwrap()),
				None,
				None,
			);
			result?;
			let n: u64 = n.try_into().unwrap();
			assert!(n <= len - offset);
			offset += n;
		}
		Ok(())
	}
	#[cfg(target_os = "freebsd")]
	{
		use nix::sys::sendfile;
		let mut offset = 0;
		while offset != len {
			let (result, n) = sendfile::sendfile(
				in_.as_raw_fd(),
				out.as_raw_fd(),
				0,
				Some((len - offset).try_into().unwrap()),
				None,
				None,
				sendfile::SfFlags::empty(),
				0,
			);
			result?;
			let n: u64 = n.try_into().unwrap();
			assert!(n <= len - offset);
			offset += n;
		}
		Ok(())
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "linux",
		target_os = "ios",
		target_os = "macos",
		target_os = "freebsd"
	)))]
	{
		let _ = (in_, out, len);
		// void *addr = mmap(NULL, length, PROT_READ, MAP_FILE | MAP_SHARED, file descriptor, offset);
		// send(socket, addr, length, 0);
		unimplemented!()
	}
}

/// Loops `splice` till len elapsed or error
#[cfg(any(target_os = "android", target_os = "linux"))]
pub fn copy_splice<O: AsRawFd, I: AsRawFd>(in_: &I, out: &O, len: u64) -> Result<(), nix::Error> {
	let mut offset = 0;
	while offset != len {
		let n = fcntl::splice(
			in_.as_raw_fd(),
			None,
			out.as_raw_fd(),
			None,
			(len - offset).try_into().unwrap(),
			fcntl::SpliceFFlags::empty(),
		)?;
		let n: u64 = n.try_into().unwrap();
		assert!(n <= len - offset);
		offset += n;
	}
	Ok(())
}

/// Returns the path of the directory that contains entries for each open file descriptor. On Linux this is `/proc/self/fd`. Doesn't work on Windows.
pub fn fd_dir() -> io::Result<path::PathBuf> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		Ok(path::PathBuf::from("/proc/self/fd"))
	}
	#[cfg(any(target_os = "freebsd", target_os = "macos", target_os = "ios"))]
	{
		Ok(path::PathBuf::from("/dev/fd"))
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "linux",
		target_os = "freebsd",
		target_os = "macos",
		target_os = "ios"
	)))]
	{
		Err(io::Error::new(
			io::ErrorKind::NotFound,
			"no known /proc/self/fd equivalent for OS",
		))
	}
}
/// Returns the path of the entry for a particular open file descriptor. On Linux this is `/proc/self/fd/{fd}`. Doesn't work on Windows.
pub fn fd_path(fd: Fd) -> io::Result<path::PathBuf> {
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		Ok(path::PathBuf::from(format!("/proc/self/fd/{}", fd)))
	}
	#[cfg(any(
		target_os = "freebsd",
		target_os = "netbsd",
		target_os = "macos",
		target_os = "ios"
	))]
	{
		Ok(path::PathBuf::from(format!("/dev/fd/{}", fd)))
	}
	#[cfg(not(any(
		target_os = "android",
		target_os = "linux",
		target_os = "freebsd",
		target_os = "netbsd",
		target_os = "macos",
		target_os = "ios"
	)))]
	{
		let _ = fd;
		Err(io::Error::new(
			io::ErrorKind::NotFound,
			"no known /proc/self/fd equivalent for OS",
		))
	}
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Iterator for all open file descriptors. Doesn't work on Windows.
///
/// # Examples
///
/// ```
/// use palaver::file::FdIter;
///
/// // Close all file descriptors except std{in,out,err}.
/// # #[cfg(unix)]
/// for fd in FdIter::new().unwrap() {
/// 	if fd > 2 {
/// 		nix::unistd::close(fd).unwrap();
/// 	}
/// }
/// ```
pub struct FdIter(#[cfg(unix)] *mut libc::DIR);
impl FdIter {
	/// Create a new FdIter. Returns Err on OSs that don't support this.
	pub fn new() -> Result<Self, io::Error> {
		let dir = fd_dir()?;
		#[cfg(unix)]
		{
			let dir =
				CString::new(<path::PathBuf as Into<OsString>>::into(dir).into_vec()).unwrap();
			let dirp: *mut libc::DIR = unsafe { libc::opendir(dir.as_ptr()) };
			assert!(!dirp.is_null());
			Ok(Self(dirp))
		}
		#[cfg(windows)]
		{
			let _ = dir;
			Err(io::Error::new(
				io::ErrorKind::NotFound,
				"can't iterate dir?",
			))
		}
	}
}
impl Iterator for FdIter {
	// https://stackoverflow.com/questions/899038/getting-the-highest-allocated-file-descriptor/918469#918469
	type Item = Fd;

	fn next(&mut self) -> Option<Self::Item> {
		#[cfg(unix)]
		{
			let mut dent;
			while {
				dent = unsafe { libc::readdir(self.0) };
				!dent.is_null()
			} {
				// https://github.com/rust-lang/rust/issues/34668
				let name = unsafe { CStr::from_ptr((*dent).d_name.as_ptr()) };
				if name == CStr::from_bytes_with_nul(b".\0").unwrap()
					|| name == CStr::from_bytes_with_nul(b"..\0").unwrap()
				{
					continue;
				}
				let fd = name
					.to_str()
					.map_err(|_| ())
					.and_then(|fd| fd.parse::<Fd>().map_err(|_| ()));
				if fd.is_err() || fd.unwrap() == unsafe { libc::dirfd(self.0) } {
					continue;
				}
				return Some(fd.unwrap());
			}
			None
		}
		#[cfg(windows)]
		{
			unreachable!()
		}
	}
}
impl fmt::Debug for FdIter {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("FdIter").finish()
	}
}
impl Drop for FdIter {
	fn drop(&mut self) {
		#[cfg(unix)]
		{
			let ret = unsafe { libc::closedir(self.0) };
			assert_eq!(ret, 0);
		}
	}
}
