//! We must not read stdin for a command that takes no request.
//!
//! `shader-sources` takes no input. If we read stdin before looking at the
//! command, the command would fail in two ways. A read from an open pipe
//! blocks until the caller's timeout, and a read from a non-blocking
//! descriptor returns EAGAIN. With a terminal, neither happens.
//!
//! The open pipe is the same on every platform, so in that case we start the
//! CLI with `std::process::Command` and keep the pipe's write end open. The
//! non-blocking descriptor exists only on POSIX systems, and there we start
//! the child with `fork` and `exec`, because Command clears `O_NONBLOCK` on
//! the descriptors it installs, after any `pre_exec` hook. With Command the
//! descriptor would block, and the test would never see the error.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn a_command_with_no_request_finishes_while_stdin_stays_open() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg("shader-sources")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start rominabox-cli");
    // We keep the write end open and never write to it, so a read of stdin blocks.
    let held = child.stdin.take();
    // Read while waiting, because a child that fills the pipe would otherwise
    // block on its output and look like a child blocked on its input.
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let reader = thread::spawn(move || {
        let mut out = String::new();
        stdout.read_to_string(&mut out).ok();
        out
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for rominabox-cli") {
            break status;
        }
        if started.elapsed() > Duration::from_secs(5) {
            child.kill().ok();
            child.wait().ok();
            panic!("shader-sources did not finish within 5 seconds while stdin was a pipe nobody writes to");
        }
        thread::sleep(Duration::from_millis(20));
    };
    drop(held);
    let out = reader.join().expect("read stdout");
    assert!(status.success(), "shader-sources failed with stdin left open: {out}");
    assert!(
        out.contains("\"type\":\"result\""),
        "shader-sources did not return its catalog: {out}"
    );
}

#[cfg(unix)]
mod nonblocking {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::thread;
    use std::time::{Duration, Instant};

    fn open_pipe() -> (OwnedFd, OwnedFd) {
        let mut fds = [0; 2];
        let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(rc, 0, "pipe");
        unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
    }

    /// The child, killed if the test ends while it is still running.
    struct Running {
        pid: i32,
    }

    impl Drop for Running {
        fn drop(&mut self) {
            if self.pid <= 0 {
                return;
            }
            unsafe {
                libc::kill(self.pid, libc::SIGKILL);
                let mut status = 0;
                libc::waitpid(self.pid, &mut status, 0);
            }
        }
    }

    impl Running {
        fn finished(&mut self) -> Option<i32> {
            let mut status = 0;
            let rc = unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) };
            if rc == 0 {
                return None;
            }
            assert_eq!(rc, self.pid, "waitpid");
            self.pid = 0;
            if libc::WIFEXITED(status) {
                Some(libc::WEXITSTATUS(status))
            } else {
                Some(status)
            }
        }
    }

    fn spawn_nonblocking() -> (Running, OwnedFd, OwnedFd) {
        let (stdin_read, stdin_write) = open_pipe();
        let (stdout_read, stdout_write) = open_pipe();
        let program = CString::new(env!("CARGO_BIN_EXE_rominabox-cli")).unwrap();
        let arg0 = CString::new("rominabox-cli").unwrap();
        let arg1 = CString::new("shader-sources").unwrap();
        let argv = [arg0.as_ptr(), arg1.as_ptr(), std::ptr::null()];
        // We take it before the fork. In a multithreaded test, the child may call
        // only async-signal-safe functions before exec, and a Rust method is not.
        let path = program.as_ptr();
        let args = argv.as_ptr();
        let stdin_r = stdin_read.as_raw_fd();
        let stdin_w = stdin_write.as_raw_fd();
        let stdout_r = stdout_read.as_raw_fd();
        let stdout_w = stdout_write.as_raw_fd();
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork");
        if pid == 0 {
            unsafe {
                if libc::dup2(stdin_r, 0) < 0 || libc::dup2(stdout_w, 1) < 0 {
                    libc::_exit(125);
                }
                let flags = libc::fcntl(0, libc::F_GETFL);
                if flags < 0 || libc::fcntl(0, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                    libc::_exit(126);
                }
                for fd in [stdin_r, stdin_w, stdout_r, stdout_w] {
                    if fd > 2 {
                        libc::close(fd);
                    }
                }
                libc::execv(path, args);
                libc::_exit(127);
            }
        }
        (Running { pid }, stdin_write, stdout_read)
    }

    fn wait_for(child: &mut Running) -> Result<i32, &'static str> {
        let started = Instant::now();
        loop {
            if let Some(code) = child.finished() {
                return Ok(code);
            }
            if started.elapsed() > Duration::from_secs(5) {
                return Err("did not finish within 5 seconds");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn read_all(fd: OwnedFd) -> String {
        let mut file = std::fs::File::from(fd);
        let mut out = String::new();
        std::io::Read::read_to_string(&mut file, &mut out).ok();
        out
    }

    #[test]
    fn a_command_with_no_request_does_not_error_on_a_nonblocking_stdin() {
        let (mut child, write, stdout) = spawn_nonblocking();
        let code = wait_for(&mut child)
            .unwrap_or_else(|why| panic!("shader-sources {why} on a non-blocking stdin"));
        drop(write);
        let out = read_all(stdout);
        assert_eq!(
            code, 0,
            "shader-sources errored on a non-blocking stdin: {out}"
        );
        assert!(
            !out.contains("could not read stdin"),
            "shader-sources reported a stdin error: {out}"
        );
    }
}
