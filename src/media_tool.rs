use std::io::{self, BufRead, BufReader};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::JoinHandle;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::process::{self as child_process, Child, ReaderError, RetainedOutput};
use crate::source::SourceSpan;

const CAPTURE_LIMIT: usize = 8 * 1024 * 1024;
const STDERR_TAIL_LIMIT: usize = 64 * 1024;

pub(crate) struct CapturedOutput {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

pub(crate) fn capture(
    mut command: Command,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
) -> Result<CapturedOutput> {
    let debug = format!("{command:?}");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = spawn(command, diagnostic, span, &debug)?;
    let stdout = child_process::take_stdout(&mut child).expect("piped media-tool stdout");
    let stderr = child_process::take_stderr(&mut child).expect("piped media-tool stderr");
    let stdout_reader =
        std::thread::spawn(move || child_process::read_prefix(stdout, CAPTURE_LIMIT));
    let stderr_reader = std::thread::spawn(move || child_process::read_tail(stderr, CAPTURE_LIMIT));
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
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = spawn(command, diagnostic, span, &debug)?;
    let stderr = child_process::take_stderr(&mut child).expect("piped media-tool stderr");
    let stderr_reader =
        std::thread::spawn(move || child_process::read_tail(stderr, STDERR_TAIL_LIMIT));
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
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = spawn(command, diagnostic, span, &debug)?;
    let stdout = child_process::take_stdout(&mut child).expect("piped media-tool stdout");
    let stderr = child_process::take_stderr(&mut child).expect("piped media-tool stderr");
    let stderr_reader =
        std::thread::spawn(move || child_process::read_tail(stderr, STDERR_TAIL_LIMIT));

    let lines = for_each_bounded_line(
        BufReader::new(stdout),
        line_limit,
        diagnostic,
        span,
        &mut visitor,
    );
    if let Err(error) = lines {
        child_process::terminate(&mut child);
        let _ = join_reader(stderr_reader, "stderr", diagnostic, span, &debug);
        return Err(error.note(debug));
    }
    let status = wait(&mut child, diagnostic, span, &debug)?;
    let stderr = join_reader(stderr_reader, "stderr", diagnostic, span, &debug)?;
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
    command: Command,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    debug: &str,
) -> Result<Child> {
    child_process::spawn(command).map_err(|error| {
        Diagnostic::builtin(
            diagnostic,
            format!("could not start external media tool: {error}"),
            span.clone(),
        )
        .note(debug)
    })
}

fn wait(
    child: &mut Child,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    debug: &str,
) -> Result<ExitStatus> {
    child_process::wait(child).map_err(|error| {
        Diagnostic::builtin(
            diagnostic,
            format!("could not wait for external media tool: {error}"),
            span.clone(),
        )
        .note(debug)
    })
}

fn join_reader(
    reader: JoinHandle<io::Result<RetainedOutput>>,
    stream: &str,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
    debug: &str,
) -> Result<RetainedOutput> {
    child_process::join_reader(reader).map_err(|error| {
        let message = match error {
            ReaderError::Panicked => format!("external media-tool {stream} reader panicked"),
            ReaderError::Io(error) => {
                format!("could not read external media-tool {stream}: {error}")
            }
        };
        Diagnostic::builtin(diagnostic, message, span.clone()).note(debug)
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

#[cfg(test)]
mod tests {
    use std::io::Read;

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

    #[cfg(unix)]
    #[test]
    fn streamed_visitor_errors_terminate_the_child_and_remain_primary() {
        use std::time::{Duration, Instant};

        let mut command = Command::new("sh");
        command.args(["-c", "printf 'line\n'; sleep 5"]);
        let span = SourceSpan::file_start("test");
        let started = Instant::now();
        let error = stream_stdout_lines(command, 256, BuiltinDiagnostic::Ffprobe, &span, |_| {
            Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "visitor stopped",
                span.clone(),
            ))
        })
        .expect_err("visitor failure");

        assert_eq!(error.code, "E_INVALID_PLAN");
        assert_eq!(error.message, "visitor stopped");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "descendant process kept the stderr pipe open"
        );
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
