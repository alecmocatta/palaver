# palaver

[![Crates.io](https://img.shields.io/crates/v/palaver.svg?maxAge=86400)](https://crates.io/crates/palaver)
[![MIT / Apache 2.0 licensed](https://img.shields.io/crates/l/palaver.svg?maxAge=2592000)](#License)
[![Build Status](https://dev.azure.com/alecmocatta/palaver/_apis/build/status/tests?branchName=master)](https://dev.azure.com/alecmocatta/palaver/_build/latest?branchName=master)

[Docs](https://docs.rs/palaver/0.3.0-alpha.1)

Cross-platform polyfills.

This library attempts to provide reliable polyfills for functionality that isn't implemented on all platforms.

`palaver` = "Platform Abstraction Layer" + pa·lav·er *n.* prolonged and tedious fuss.

## Functionality

<table><!-- https://github.com/alecmocatta/palaver/new/master to preview changes -->
<tr><th>Threading</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/thread/fn.gettid.html"><code>gettid()</code><a></td><td>Get thread ID</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/thread/fn.count.html"><code>count()</code></a></td><td>Number of threads in current process</td><td>✓</td><td>✓</td><td> </td><td> </td><td> </td><td>✓</td><td>✓</td></tr>
<tr><th>Files</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.seal_fd.html"><code>seal_fd()</code></a></td><td>Make a file descriptor read-only</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.dup_fd.html"><code>dup_fd()</code></a></td><td>Duplicate a file descriptor</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.copy_fd.html"><code>copy_fd()</code></a></td><td>Copy a file descriptor to a specific offset</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.move_fd.html"><code>move_fd()</code></a></td><td>Move a file descriptor to a specific offset</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.move_fds.html"><code>move_fds()</code></a></td><td>Move file descriptors to specific offsets</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.fd_dir.html"><code>fd_dir()</code></a></td><td>Get a path to the file descriptor directory</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.fd_path.html"><code>fd_path()</code></a></td><td>Get a path to a file descriptor</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/struct.FdIter.html"><code>FdIter</code></a></td><td>Iterate all open file descriptors</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.memfd_create.html"><code>memfd_create()</code></a></td><td>Create an anonymous file</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.fexecve.html"><code>fexecve()</code></a></td><td>Execute program specified via file descriptor</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.copy.html"><code>copy()</code></a></td><td>Copy by looping <code>io::copy</code></td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.copy_sendfile.html"><code>copy_sendfile()</code></a></td><td>Copy using <code>sendfile</code></td><td>✓</td><td>✓</td><td> </td><td>✓</td><td> </td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.copy_splice.html"><code>copy_splice()</code></a></td><td>Copy using <code>splice</code></td><td>✓</td><td> </td><td> </td><td> </td><td> </td><td> </td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/file/fn.pipe.html"><code>pipe()</code></a></td><td>Create a pipe</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><th>Socket</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/socket/fn.socket.html"><code>socket()</code></a></td><td>Create a socket</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/socket/fn.accept.html"><code>accept()</code></a></td><td>Accept a connection on a socket</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/socket/fn.is_connected.html"><code>is_connected()</code></a></td><td>Get whether a pending connection is connected</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/socket/fn.unreceived.html"><code>unreceived()</code></a></td><td>Get number of bytes readable</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/socket/fn.unsent.html"><code>unsent()</code></a></td><td>Get number of bytes that have yet to be acknowledged</td><td>✓</td><td>✓</td><td> </td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><th>Env</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/env/fn.exe.html"><code>exe()</code></a></td><td>Opens the current running executable</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/env/fn.exe_path.html"><code>exe_path()</code></a></td><td>Get a path to the current running executable</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/env/fn.args.html"><code>args()</code></a></td><td>Get command line arguments</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/env/fn.vars.html"><code>vars()</code></a></td><td>Get environment variables</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><th>Process</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/process/fn.count.html"><code>count()</code></a></td><td>Count the processes visible to the current process</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/process/fn.count_threads.html"><code>count_threads()</code></a></td><td>Count the threads visible to the current process</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/process/fn.fork.html"><code>fork()</code></a></td><td>Fork a process, using process descriptors where available</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><th>Valgrind</th><th>Description</th><th>Linux</th><th>macOS</th><th>Windows</th><th>FreeBSD</th><th>NetBSD</th><th>iOS</th><th>Android</th></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/valgrind/fn.is.html"><code>is()</code></a></td><td>Check if running under Valgrind</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
<tr><td><a href="https://docs.rs/palaver/0.3.0-alpha.1/palaver/valgrind/fn.start_fd.html"><code>start_fd()</code></a></td><td>Get Valgrind's file descriptor range</td><td>✓</td><td>✓</td><td>–</td><td>✓</td><td>✓</td><td>✓</td><td>✓</td></tr>
</table>

## License
Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE.txt](LICENSE-APACHE.txt) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT.txt](LICENSE-MIT.txt) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
