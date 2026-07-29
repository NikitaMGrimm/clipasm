use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

const CAPTURE_LIMIT: usize = 8 * 1024 * 1024;
const STDERR_TAIL_LIMIT: usize = 64 * 1024;
const START_ATTEMPTS: usize = 5;
const START_RETRY_DELAY: Duration = Duration::from_millis(10);

pub(crate) struct CapturedOutput {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct RetainedOutput {
    pub(crate) bytes: Vec<u8>,
    pub(crate) truncated: bool,
}

pub(crate) fn capture(
    mut command: Command,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
) -> Result<CapturedOutput> {
    let debug = format!("{command:?}");
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn(command, diagnostic, span, &debug)?;
    let stdout = child.stdout.take().expect("piped media-tool stdout");
    let stderr = child.stderr.take().expect("piped media-tool stderr");
    let stdout_reader = std::thread::spawn(move || read_prefix(stdout, CAPTURE_LIMIT));
    let stderr_reader = std::thread::spawn(move || read_tail(stderr, CAPTURE_LIMIT));
    let status = wait(&mut child, diagnostic, span, &debug);
    let stdout = join_reader(stdout_reader, "stdout", diagnostic, span, &debug);
    let stderr = join_reader(stderr_reader, "stderr", diagnostic, span, &debug);
    let status = status?;
    let stdout = stdout?;
    let stderr = stderr?;

    if !status.success() {
        return Err(exit_diagnostic(
            diagnostic,
            span,
            &debug,
            status,
            &stderr.bytes,
            stderr.truncated,
            CAPTURE_LIMIT,
        ));
    }
    if stdout.truncated {
        return Err(output_limit_diagnostic(
            diagnostic,
            span,
            &debug,
            "stdout",
            CAPTURE_LIMIT,
        ));
    }
    if stderr.truncated {
        return Err(output_limit_diagnostic(
            diagnostic,
            span,
            &debug,
            "stderr",
            CAPTURE_LIMIT,
        ));
    }

    Ok(CapturedOutput {
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

pub(crate) fn run(
    mut command: Command,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
) -> Result<()> {
    let debug = format!("{command:?}");
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = spawn(command, diagnostic, span, &debug)?;
    let stderr = child.stderr.take().expect("piped media-tool stderr");
    let stderr_reader = std::thread::spawn(move || read_tail(stderr, STDERR_TAIL_LIMIT));
    let status = wait(&mut child, diagnostic, span, &debug);
    let stderr = join_reader(stderr_reader, "stderr", diagnostic, span, &debug);
    let status = status?;
    let stderr = stderr?;
    if !status.success() {
        return Err(exit_diagnostic(
            diagnostic,
            span,
            &debug,
            status,
            &stderr.bytes,
            stderr.truncated,
            STDERR_TAIL_LIMIT,
        ));
    }
    Ok(())
}

pub(crate) fn stream_stdout_lines(
    mut command: Command,
    line_limit: usize,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    mut visitor: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let debug = format!("{command:?}");
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn(command, diagnostic, span, &debug)?;
    let stdout = child.stdout.take().expect("piped media-tool stdout");
    let stderr = child.stderr.take().expect("piped media-tool stderr");
    let stderr_reader = std::thread::spawn(move || read_tail(stderr, STDERR_TAIL_LIMIT));

    let lines = for_each_bounded_line(
        BufReader::new(stdout),
        line_limit,
        diagnostic,
        span,
        &mut visitor,
    );
    if lines.is_err() {
        let _ = child.kill();
    }
    let status = wait(&mut child, diagnostic, span, &debug);
    let stderr = join_reader(stderr_reader, "stderr", diagnostic, span, &debug);
    let status = status?;
    let stderr = stderr?;

    if let Err(error) = lines {
        return Err(error.note(debug));
    }
    if !status.success() {
        return Err(exit_diagnostic(
            diagnostic,
            span,
            &debug,
            status,
            &stderr.bytes,
            stderr.truncated,
            STDERR_TAIL_LIMIT,
        ));
    }
    Ok(())
}

fn spawn(
    mut command: Command,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    debug: &str,
) -> Result<Child> {
    for attempt in 1..=START_ATTEMPTS {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) if executable_is_temporarily_busy(&error) && attempt < START_ATTEMPTS => {
                std::thread::sleep(START_RETRY_DELAY);
            }
            Err(error) => {
                return Err(Diagnostic::builtin(
                    diagnostic,
                    format!("could not start external media tool: {error}"),
                    span.clone(),
                )
                .note(debug));
            }
        }
    }
    unreachable!("the final media-tool start attempt always returns")
}

fn wait(
    child: &mut Child,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    debug: &str,
) -> Result<ExitStatus> {
    match child.wait() {
        Ok(status) => Ok(status),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(Diagnostic::builtin(
                diagnostic,
                format!("could not wait for external media tool: {error}"),
                span.clone(),
            )
            .note(debug))
        }
    }
}

fn join_reader(
    reader: JoinHandle<io::Result<RetainedOutput>>,
    stream: &str,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    debug: &str,
) -> Result<RetainedOutput> {
    reader
        .join()
        .map_err(|_| {
            Diagnostic::builtin(
                diagnostic,
                format!("external media-tool {stream} reader panicked"),
                span.clone(),
            )
            .note(debug)
        })?
        .map_err(|error| {
            Diagnostic::builtin(
                diagnostic,
                format!("could not read external media-tool {stream}: {error}"),
                span.clone(),
            )
            .note(debug)
        })
}

fn read_prefix(mut reader: impl Read, limit: usize) -> io::Result<RetainedOutput> {
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

fn for_each_bounded_line(
    mut reader: impl BufRead,
    limit: usize,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    visitor: &mut impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let mut line = Vec::with_capacity(limit.min(256));
    let mut overflow = false;
    loop {
        let available = reader.fill_buf().map_err(|error| {
            Diagnostic::builtin(
                diagnostic,
                format!("could not read streamed media-tool output: {error}"),
                span.clone(),
            )
        })?;
        if available.is_empty() {
            if overflow {
                return Err(line_limit_diagnostic(limit, span));
            }
            if !line.is_empty() {
                visitor(&line)?;
            }
            return Ok(());
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let segment_end = newline.unwrap_or(available.len());
        if !overflow {
            let remaining = limit.saturating_sub(line.len());
            let retained = segment_end.min(remaining);
            line.extend_from_slice(&available[..retained]);
            overflow = retained < segment_end;
        }
        let consumed = segment_end + usize::from(newline.is_some());
        reader.consume(consumed);
        if newline.is_some() {
            if overflow {
                return Err(line_limit_diagnostic(limit, span));
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            visitor(&line)?;
            line.clear();
        }
    }
}

fn line_limit_diagnostic(limit: usize, span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::ToolOutputLimit,
        format!("external media-tool output line exceeds the {limit}-byte limit"),
        span.clone(),
    )
}

fn exit_diagnostic(
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    debug: &str,
    status: ExitStatus,
    stderr: &[u8],
    truncated: bool,
    retained_limit: usize,
) -> Diagnostic {
    let stderr = String::from_utf8_lossy(stderr);
    let marker = if truncated {
        format!("[stderr truncated to final {retained_limit} bytes]\n")
    } else {
        String::new()
    };
    Diagnostic::builtin(
        diagnostic,
        format!(
            "external media tool exited with {status}\n{marker}{}",
            stderr.trim()
        ),
        span.clone(),
    )
    .note(debug)
}

fn output_limit_diagnostic(
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    debug: &str,
    stream: &str,
    limit: usize,
) -> Diagnostic {
    Diagnostic::builtin(
        diagnostic,
        format!("external media-tool {stream} exceeds the {limit}-byte capture limit"),
        span.clone(),
    )
    .note(debug)
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
    fn stderr_tail_keeps_only_the_final_bytes() {
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

    #[test]
    fn streamed_lines_do_not_accumulate_the_complete_output() {
        let reader = BufReader::with_capacity(
            17,
            RepeatingReader {
                pattern: b"1024\n",
                repetitions: 100_000,
                offset: 0,
            },
        );
        let mut lines = 0_u64;
        let span = SourceSpan::file_start("test");
        for_each_bounded_line(
            reader,
            16,
            BuiltinDiagnostic::ToolOutputLimit,
            &span,
            &mut |line| {
                assert_eq!(line, b"1024");
                lines += 1;
                Ok(())
            },
        )
        .expect("streamed lines");
        assert_eq!(lines, 100_000);
    }

    #[test]
    fn streamed_lines_reject_one_overlong_record() {
        let reader = BufReader::new(std::io::Cursor::new(vec![b'x'; 257]));
        let span = SourceSpan::file_start("test");
        let error = for_each_bounded_line(
            reader,
            256,
            BuiltinDiagnostic::ToolOutputLimit,
            &span,
            &mut |_| Ok(()),
        )
        .expect_err("long line");
        assert_eq!(error.code, "E_TOOL_OUTPUT_LIMIT");
    }
}
