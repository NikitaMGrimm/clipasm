//! Shared child-process lifecycle and bounded pipe retention.
//!
//! Command construction, protocols, and diagnostics remain owned by their
//! phase adapters. This module owns the operating-system lifecycle invariants
//! those adapters must share: retrying a temporarily busy executable, draining
//! bounded output, joining reader threads, and killing plus reaping after a
//! failed wait.

use std::collections::VecDeque;
use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus};
use std::thread::JoinHandle;
use std::time::Duration;

const START_ATTEMPTS: usize = 5;
const START_RETRY_DELAY: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(crate) struct RetainedOutput {
    pub(crate) bytes: Vec<u8>,
    pub(crate) truncated: bool,
}

#[derive(Debug)]
pub(crate) enum ReaderError {
    Panicked,
    Io(io::Error),
}

pub(crate) fn spawn(mut command: Command) -> io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;

        command.process_group(0);
    }
    for attempt in 1..=START_ATTEMPTS {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) if executable_is_temporarily_busy(&error) && attempt < START_ATTEMPTS => {
                std::thread::sleep(START_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the final process start attempt always returns")
}

pub(crate) fn wait(child: &mut Child) -> io::Result<ExitStatus> {
    let status = wait_with(child, Child::wait)?;
    terminate_group(child);
    Ok(status)
}

pub(crate) fn terminate(child: &mut Child) {
    terminate_group(child);
    let _ = child.kill();
    let _ = child.wait();
}

fn terminate_group(child: &Child) {
    #[cfg(unix)]
    {
        let group = rustix::process::Pid::from_child(child);
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    #[cfg(not(unix))]
    {
        let _ = child;
    }
}

pub(crate) fn join_reader(
    reader: JoinHandle<io::Result<RetainedOutput>>,
) -> Result<RetainedOutput, ReaderError> {
    reader
        .join()
        .map_err(|_| ReaderError::Panicked)?
        .map_err(ReaderError::Io)
}

pub(crate) fn read_prefix(mut reader: impl Read, limit: usize) -> io::Result<RetainedOutput> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = vec![0_u8; 16 * 1024].into_boxed_slice();
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = read.min(remaining);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
    Ok(RetainedOutput { bytes, truncated })
}

pub(crate) fn read_tail(mut reader: impl Read, limit: usize) -> io::Result<RetainedOutput> {
    let mut bytes = VecDeque::with_capacity(limit.min(64 * 1024));
    let mut buffer = vec![0_u8; 16 * 1024].into_boxed_slice();
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        if read >= limit {
            bytes.clear();
            bytes.extend(&buffer[read - limit..read]);
            truncated = true;
            continue;
        }
        let overflow = bytes.len().saturating_add(read).saturating_sub(limit);
        if overflow > 0 {
            bytes.drain(..overflow);
            truncated = true;
        }
        bytes.extend(&buffer[..read]);
    }
    Ok(RetainedOutput {
        bytes: bytes.into_iter().collect(),
        truncated,
    })
}

fn wait_with(
    child: &mut Child,
    wait: impl FnOnce(&mut Child) -> io::Result<ExitStatus>,
) -> io::Result<ExitStatus> {
    match wait(child) {
        Ok(status) => Ok(status),
        Err(error) => {
            terminate(child);
            Err(error)
        }
    }
}

fn executable_is_temporarily_busy(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(26)
    }
    #[cfg(not(unix))]
    {
        let _ = error;
        false
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    struct RepeatingReader {
        pattern: &'static [u8],
        repetitions: usize,
        offset: usize,
    }

    impl Read for RepeatingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.repetitions == 0 || buffer.is_empty() {
                return Ok(0);
            }
            let mut written = 0;
            while written < buffer.len() && self.repetitions > 0 {
                let remaining = &self.pattern[self.offset..];
                let count = remaining.len().min(buffer.len() - written);
                buffer[written..written + count].copy_from_slice(&remaining[..count]);
                written += count;
                self.offset += count;
                if self.offset == self.pattern.len() {
                    self.offset = 0;
                    self.repetitions -= 1;
                }
            }
            Ok(written)
        }
    }

    #[test]
    fn prefix_capture_drains_without_growing_past_the_limit() {
        let output = read_prefix(
            RepeatingReader {
                pattern: b"0123456789",
                repetitions: 10_000,
                offset: 0,
            },
            128,
        )
        .expect("bounded prefix");
        assert_eq!(output.bytes.len(), 128);
        assert!(output.truncated);
    }

    #[test]
    fn tail_capture_keeps_only_the_final_bytes() {
        let output = read_tail(
            RepeatingReader {
                pattern: b"0123456789",
                repetitions: 10_000,
                offset: 0,
            },
            11,
        )
        .expect("bounded tail");
        assert_eq!(output.bytes, b"90123456789");
        assert!(output.truncated);
    }

    #[cfg(unix)]
    #[test]
    fn successful_wait_terminates_descendants_holding_pipes() {
        use std::process::Stdio;
        use std::time::{Duration, Instant};

        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 5 & exit 0"])
            .stderr(Stdio::piped());
        let mut child = spawn(command).expect("shell child");
        let stderr = child.stderr.take().expect("stderr pipe");
        let reader = std::thread::spawn(move || read_tail(stderr, 1024));
        let started = Instant::now();

        let status = wait(&mut child).expect("successful wait");
        let retained = join_reader(reader).expect("closed descendant pipe");

        assert!(status.success());
        assert!(retained.bytes.is_empty());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "successful parent left a descendant holding stderr open"
        );
    }

    #[cfg(unix)]
    #[test]
    fn wait_failure_kills_and_reaps_the_child() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 60"])
            .spawn()
            .expect("sleeping child");

        let error = wait_with(&mut child, |_| {
            Err(io::Error::other("injected wait failure"))
        })
        .expect_err("injected wait failure");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(
            child.try_wait().expect("reaped child status").is_some(),
            "child must be reaped before returning"
        );
    }
}
