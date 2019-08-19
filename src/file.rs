//! File and file descriptor-related functionality

use super::*;
#[cfg(unix)]
use ext::ToHex;
#[cfg(unix)]
use nix::{errno, fcntl, sys::stat, unistd};
#[cfg(any(
	target_os = "linux",
	target_os = "android",
	target_os = "macos",
	target_os = "ios",
	target_os = "freebsd"
))]
use std::convert::TryInto;
use std::{
	convert::Infallible, fmt, io::{self, Read, Write}, iter, path
};
#[cfg(unix)]
use std::{
	ffi::{CStr, CString, OsString}, fs, mem, os::unix::ffi::OsStringExt, os::unix::io::AsRawFd, os::unix::io::FromRawFd
};

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
	let fl_flags =
		fcntl::OFlag::from_bits_truncate(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFL).unwrap())
			& !(fcntl::OFlag::O_WRONLY | fcntl::OFlag::O_RDWR)
			| fcntl::OFlag::O_RDONLY;
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
				let mut flags = fcntl::OFlag::from_bits_truncate(
					fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFL).unwrap(),
				);
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
		let mut name = tmpfile(&"/".into()); // ENAMETOOLONG on mac for >31 byte path component https://github.com/apple/darwin-xnu/blob/a449c6a3b8014d9406c2ddbdc81795da24aa7443/bsd/kern/posix_shm.c#L94
		let name = heapless_string_to_cstr(&mut name);
		mman::shm_open(
			name,
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
			mman::shm_unlink(name).unwrap();
			fd
		})
	});
	#[cfg(unix)]
	{
		ret.or_else(|_e| {
			let mut name = tmpfile(&"/tmp/".into());
			let name = heapless_string_to_cstr(&mut name);
			fcntl::open(
				name,
				fcntl::OFlag::O_RDWR | fcntl::OFlag::O_CREAT | fcntl::OFlag::O_EXCL,
				stat::Mode::S_IRWXU,
			)
			.map(|fd| {
				unistd::unlink(name).unwrap();
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

/// `execve`, not requiring memory allocation unlike nix's, but panics on >255 args or vars.
#[cfg(unix)]
pub fn execve(path: &CStr, args: &[&CStr], vars: &[&CStr]) -> nix::Result<Infallible> {
	let args: heapless::Vec<*const libc::c_char, heapless::consts::U256> = args
		.iter()
		.map(|arg| arg.as_ptr())
		.chain(iter::once(std::ptr::null()))
		.collect();
	let vars: heapless::Vec<*const libc::c_char, heapless::consts::U256> = vars
		.iter()
		.map(|arg| arg.as_ptr())
		.chain(iter::once(std::ptr::null()))
		.collect();

	let _ = unsafe { libc::execve(path.as_ptr(), args.as_ptr(), vars.as_ptr()) };

	Err(nix::Error::Sys(nix::errno::Errno::last()))
}

fn heapless_string_to_cstr<N>(string: &mut heapless::String<N>) -> &CStr
where
	N: heapless::ArrayLength<u8>,
{
	string.push('\0').unwrap();
	CStr::from_bytes_with_nul(string.as_bytes()).unwrap()
}

#[cfg(unix)]
fn tmpfile(
	prefix: &heapless::String<heapless::consts::U6>,
) -> heapless::String<typenum::operator_aliases::Sum<heapless::consts::U6, heapless::consts::U32>> {
	let mut random: [u8; 16] = unsafe { mem::uninitialized() };
	// thread_rng uses tls, might permanently open /dev/urandom, which may have undesirable side effects
	// let rand = fs::File::open("/dev/urandom").expect("Couldn't open /dev/urandom");
	let rand = nix::fcntl::open(
		"/dev/urandom",
		nix::fcntl::OFlag::O_RDONLY,
		nix::sys::stat::Mode::empty(),
	)
	.expect("Couldn't open /dev/urandom");
	let rand = unsafe { fs::File::from_raw_fd(rand) };
	(&rand).read_exact(&mut random).unwrap();
	drop(rand);
	let mut ret = heapless::String::new();
	std::fmt::Write::write_fmt(&mut ret, format_args!("{}{}", prefix, random.to_hex())).unwrap();
	ret
}

/// Falls back to execve("/proc/self/fd/{fd}",...), falls back to execve("/tmp/{randomfilename}")
#[cfg(unix)]
pub fn fexecve(fd: Fd, arg: &[&CStr], env: &[&CStr]) -> nix::Result<Infallible> {
	#[cfg(any(target_os = "android", target_os = "freebsd", target_os = "linux"))]
	{
		unistd::fexecve(fd, arg, env)
	}
	#[cfg(all(
		unix,
		not(any(target_os = "android", target_os = "freebsd", target_os = "linux"))
	))]
	{
		// Things tried but not helping on Mac:
		// extern "C" {
		// 	#[cfg_attr(
		// 		all(target_os = "macos", target_arch = "x86"),
		// 		link_name = "lchmod$UNIX2003"
		// 	)]
		// 	pub fn lchmod(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int;
		// }
		// let res = unsafe { libc::fchmod(fd, stat::Mode::S_IRWXU.bits() as libc::mode_t) };
		// assert_eq!(res, 0);
		// nix::errno::Errno::result(res).map(drop).unwrap();
		// const O_SYMLINK: i32 = 0x200000;
		// let x = unsafe { libc::open(path.as_ptr(), O_SYMLINK) };
		// assert_ne!(x, -1);
		// let res = unsafe { libc::fchmod(x, stat::Mode::S_IRWXU.bits() as libc::mode_t) };
		// assert_eq!(res, 0);
		// nix::errno::Errno::result(res).map(drop).unwrap();
		// let res = unsafe { lchmod(path.as_ptr(), stat::Mode::S_IRWXU.bits() as libc::mode_t) };
		// assert_eq!(res, 0);
		// nix::errno::Errno::result(res).map(drop).unwrap();
		// println!("{:?}", nix::sys::stat::stat(&*path).unwrap());
		// println!("{:?}", nix::sys::stat::lstat(&*path).unwrap());
		// let to_path_cstr = CString::new(<OsString as OsStringExt>::into_vec(to_path.clone().into())).unwrap();
		// let res = unsafe { libc::linkat(libc::AT_FDCWD, to_path_cstr.as_ptr(), libc::AT_FDCWD, path.as_ptr(), libc::AT_SYMLINK_FOLLOW) };
		// nix::errno::Errno::result(res).map(drop)?;

		let mut path = fd_path_heapless(fd).unwrap();
		let path = heapless_string_to_cstr(&mut path);
		execve(&path, arg, env).or_else(|_e| {
			let mut to_path = tmpfile(&"/tmp/".into());
			let to_path = heapless_string_to_cstr(&mut to_path);
			let to = fcntl::open(
				to_path,
				fcntl::OFlag::O_RDWR | fcntl::OFlag::O_CREAT | fcntl::OFlag::O_EXCL,
				stat::Mode::S_IRWXU,
			)
			.map(|fd| {
				if true {
					// cloexec
					let mut flags_ = fcntl::FdFlag::from_bits(
						fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFD).unwrap(),
					)
					.unwrap();
					flags_.insert(fcntl::FdFlag::FD_CLOEXEC);
					let _ = fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFD(flags_)).unwrap();
				}
				fd
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
				execve(to_path, arg, env).map_err(|e| {
					let _ = unistd::write(write, &[0]).unwrap();
					unistd::close(write).unwrap();
					unistd::unlink(to_path).unwrap();
					e
				})
			} else {
				unistd::close(write).unwrap();
				match unistd::read(read, &mut [0, 0]) {
					Ok(1) => unsafe { libc::_exit(0) },
					Ok(0) => {
						// constellation currently relies upon current_exe() on mac not having been deleted
						// unistd::unlink(to_path.as_path()).unwrap();
						unsafe { libc::_exit(0) }
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

/// Returns the path of the entry for a particular open file descriptor. On Linux this is `/proc/self/fd/{fd}`. Doesn't work on Windows.
fn fd_path_heapless(fd: Fd) -> io::Result<heapless::String<heapless::consts::U24>> {
	let mut ret = heapless::String::new();
	#[cfg(any(target_os = "android", target_os = "linux"))]
	{
		use std::fmt::Write;
		ret.write_fmt(format_args!("/proc/self/fd/{}", fd)).unwrap();
	}
	#[cfg(any(
		target_os = "freebsd",
		target_os = "netbsd",
		target_os = "macos",
		target_os = "ios"
	))]
	{
		use std::fmt::Write;
		ret.write_fmt(format_args!("/dev/fd/{}", fd)).unwrap();
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
		return Err(io::Error::new(
			io::ErrorKind::NotFound,
			"no known /proc/self/fd equivalent for OS",
		));
	}
	Ok(ret)
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
