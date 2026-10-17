use std::{
    io::{ErrorKind, Read as _},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) struct Server(Child);

impl Server {
    pub(super) fn start() -> (Self, String) {
        let mut server = Self(
            Command::new("Xvfb")
                .args([
                    "-displayfd",
                    "1",
                    "-nolisten",
                    "tcp",
                    "-noreset",
                    "-screen",
                    "0",
                    "64x64x24",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .spawn()
                .expect("install Xvfb to run the isolated X11 server contract"),
        );
        let mut output = server.0.stdout.take().unwrap();
        let flags = rustix::fs::fcntl_getfl(&output).unwrap();
        rustix::fs::fcntl_setfl(&output, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut number = String::new();
        loop {
            let mut byte = [0];
            match output.read(&mut byte) {
                Ok(1) if byte[0] == b'\n' => break,
                Ok(1) => {
                    assert!(byte[0].is_ascii_digit() && number.len() < 10);
                    number.push(char::from(byte[0]));
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                result => panic!("Xvfb did not publish a display number: {result:?}"),
            }
            assert!(
                server.0.try_wait().unwrap().is_none(),
                "Xvfb exited during startup"
            );
            assert!(Instant::now() < deadline, "Xvfb startup timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!number.is_empty());
        (server, format!(":{number}"))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = rustix::process::kill_process(
            rustix::process::Pid::from_child(&self.0),
            rustix::process::Signal::TERM,
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
