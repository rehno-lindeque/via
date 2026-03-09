use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::prompt;
use crate::session;

/// Default timeout for --until polling (seconds)
pub const DEFAULT_TIMEOUT: f64 = 30.0;

#[derive(Debug)]
enum TailMode {
    Plain,
    Since(String),
    Delim(String),
}

struct TailOptions {
    mode: TailMode,
    lines: Option<usize>,
    follow: bool,
    until: Option<String>,
    timeout_secs: Option<f64>,
}

/// Parse tail arguments
fn parse_tail_args(session: &str, args: &[String]) -> Result<TailOptions> {
    let mut mode = TailMode::Plain;
    let mut lines: Option<usize> = None;
    let mut follow = false;
    let mut until: Option<String> = None;
    let mut timeout_secs: Option<f64> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-f" => {
                follow = true;
                i += 1;
            }
            "-n" => {
                if i + 1 >= args.len() {
                    anyhow::bail!("usage: via {} tail -n <N> [--since PROMPT|--delim PROMPT|-f]", session);
                }
                lines = Some(args[i + 1].parse()
                    .with_context(|| format!("invalid number: {}", args[i + 1]))?);
                i += 2;
            }
            "--since" => {
                if !matches!(mode, TailMode::Plain) {
                    anyhow::bail!("only one of --since/--delim allowed");
                }
                let (val, consumed) = session::resolve_delim(session, args, i)?;
                mode = TailMode::Since(val);
                i += consumed;
            }
            "--delim" => {
                if !matches!(mode, TailMode::Plain) {
                    anyhow::bail!("only one of --since/--delim allowed");
                }
                let (val, consumed) = session::resolve_delim(session, args, i)?;
                mode = TailMode::Delim(val);
                i += consumed;
            }
            "--until" => {
                let (val, consumed) = session::resolve_delim(session, args, i)?;
                until = Some(val);
                i += consumed;
            }
            "--timeout" => {
                if i + 1 >= args.len() {
                    anyhow::bail!("--timeout requires a number");
                }
                timeout_secs = Some(args[i + 1].parse()
                    .with_context(|| format!("invalid timeout: {}", args[i + 1]))?);
                i += 2;
            }
            _ => break,
        }
    }

    // Validate combinations
    if follow && !matches!(mode, TailMode::Plain) {
        anyhow::bail!("-f cannot be combined with --since or --delim");
    }

    if follow && until.is_some() {
        anyhow::bail!("-f cannot be combined with --until");
    }

    if timeout_secs.is_some() && until.is_none() {
        anyhow::bail!("--timeout requires --until");
    }

    if until.is_some() && matches!(mode, TailMode::Delim(_)) {
        anyhow::bail!("--until cannot be combined with --delim");
    }

    if matches!(mode, TailMode::Plain) && lines.is_none() && !follow && until.is_none() {
        anyhow::bail!("usage: via <session> tail [-n N] (--since PROMPT | --delim PROMPT | --until PROMPT | -f)");
    }

    Ok(TailOptions { mode, lines, follow, until, timeout_secs })
}

/// Tail a session's output
pub fn tail_session(session: &str, args: &[String]) -> Result<()> {
    let opts = parse_tail_args(session, args)?;
    let stdout_path = session::stdout_path(session)?;

    // --until mode: stream output until pattern matches
    if let Some(ref pattern) = opts.until {
        let timeout = opts.timeout_secs.unwrap_or(DEFAULT_TIMEOUT);

        // If combined with --since, find the starting position from last prompt occurrence
        let start_pos = if let TailMode::Since(ref since_prompt) = opts.mode {
            find_since_position(session, since_prompt, opts.lines.unwrap_or(100))?
        } else {
            // Start from beginning so existing content is checked too
            0
        };

        return follow_until(session, pattern, timeout, start_pos, &mut std::io::stdout());
    }

    if !stdout_path.exists() {
        anyhow::bail!("no stdout at {}", stdout_path.display());
    }

    if opts.follow {
        // Use external tail -f command
        let mut cmd = Command::new("tail");
        cmd.arg("-f");
        if let Some(n) = opts.lines {
            cmd.arg("-n").arg(n.to_string());
        }
        cmd.arg(&stdout_path);

        let status = cmd.status()
            .with_context(|| "failed to execute tail -f")?;

        if !status.success() {
            anyhow::bail!("tail -f failed");
        }

        return Ok(());
    }

    match opts.mode {
        TailMode::Plain => {
            // Simple tail -n N
            let n = opts.lines.unwrap(); // Already validated
            let output = Command::new("tail")
                .arg("-n")
                .arg(n.to_string())
                .arg(&stdout_path)
                .output()
                .with_context(|| "failed to execute tail")?;

            Write::write_all(&mut std::io::stdout(), &output.stdout)?;
        }
        TailMode::Since(prompt) => {
            tail_since(session, &prompt, opts.lines.unwrap_or(100))?;
        }
        TailMode::Delim(prompt) => {
            tail_delim_internal(session, &prompt, opts.lines.unwrap_or(100))?;
        }
    }

    Ok(())
}

/// Stream output from `start_pos` until `pattern` appears in a line, writing to `writer`.
/// Waits for the stdout file to exist if it doesn't yet.
pub fn follow_until(
    session: &str,
    pattern: &str,
    timeout_secs: f64,
    start_pos: u64,
    writer: &mut dyn Write,
) -> Result<()> {
    let stdout_path = session::stdout_path(session)?;
    let poll_interval = Duration::from_millis(100);
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_secs);
    let mut pos = start_pos;

    loop {
        if Instant::now() > deadline {
            anyhow::bail!("timeout waiting for '{}' (after {}s)", pattern, timeout_secs);
        }

        // Wait for file to exist
        if !stdout_path.exists() {
            thread::sleep(poll_interval);
            continue;
        }

        let mut file = match File::open(&stdout_path) {
            Ok(f) => f,
            Err(_) => {
                thread::sleep(poll_interval);
                continue;
            }
        };

        use std::io::{Seek, SeekFrom};
        let file_size = file.seek(SeekFrom::End(0))?;

        if file_size <= pos {
            thread::sleep(poll_interval);
            continue;
        }

        // Read new content
        file.seek(SeekFrom::Start(pos))?;
        let reader = BufReader::new(&file);
        for line_result in reader.lines() {
            let line = line_result.with_context(|| "failed to read line from stdout")?;
            writeln!(writer, "{}", line)?;

            // Check if this line contains the pattern (strip ANSI for matching)
            if prompt::strip_ansi(line.as_bytes()).contains(pattern) {
                return Ok(());
            }
        }

        // Update position to current end
        pos = file_size;
        thread::sleep(poll_interval);
    }
}

/// Find the byte position of the last occurrence of a prompt in the stdout file.
/// Used by `--since` combined with `--until` to determine where to start streaming.
fn find_since_position(session: &str, prompt: &str, window: usize) -> Result<u64> {
    let stdout_path = session::stdout_path(session)?;

    if !stdout_path.exists() {
        return Ok(0);
    }

    let file = File::open(&stdout_path)
        .with_context(|| format!("failed to open {}", stdout_path.display()))?;

    let lines = read_back_until(&file, window, |lines| {
        lines.iter().any(|line| prompt::strip_ansi(line.as_bytes()).contains(prompt))
    })?;

    // Find byte offset: we need to figure out where in the file the matching line starts.
    // Read backwards from the end to find the prompt line's byte position.
    use std::io::{Seek, SeekFrom};
    let mut file = file.try_clone()?;
    let file_size = file.seek(SeekFrom::End(0))?;

    // Find the last occurrence of prompt in lines
    if let Some(idx) = lines.iter().rposition(|line| prompt::strip_ansi(line.as_bytes()).contains(prompt)) {
        // Estimate: sum bytes of lines after the match to get offset from end
        let bytes_after: usize = lines[idx..].iter()
            .map(|l| l.len() + 1) // +1 for newline
            .sum();
        let start = if bytes_after as u64 >= file_size { 0 } else { file_size - bytes_after as u64 };
        Ok(start)
    } else {
        // No prompt found, start from current end
        Ok(file_size)
    }
}

/// Tail since the last occurrence of a prompt (includes prompt)
fn tail_since(session: &str, prompt: &str, window: usize) -> Result<()> {
    let stdout_path = session::stdout_path(session)?;

    let file = File::open(&stdout_path)
        .with_context(|| format!("failed to open {}", stdout_path.display()))?;

    let lines = read_back_until(&file, window, |lines| {
        lines.iter().any(|line| prompt::strip_ansi(line.as_bytes()).contains(prompt))
    })?;

    // Find the last occurrence of prompt (strip ANSI before matching)
    if let Some(idx) = lines.iter().rposition(|line| prompt::strip_ansi(line.as_bytes()).contains(prompt)) {
        // Print from that line onwards
        for line in &lines[idx..] {
            println!("{}", line);
        }
    }

    Ok(())
}

/// Tail the last stanza delimited by prompt (from second-to-last prompt, excluding last prompt)
fn tail_delim_internal(session: &str, prompt: &str, window: usize) -> Result<()> {
    let stdout_path = session::stdout_path(session)?;

    let file = File::open(&stdout_path)
        .with_context(|| format!("failed to open {}", stdout_path.display()))?;

    let lines = read_back_until(&file, window, |lines| {
        // Need at least two prompt occurrences for a complete stanza
        lines.iter().filter(|line| prompt::strip_ansi(line.as_bytes()).contains(prompt)).count() >= 2
    })?;

    // Find all occurrences of prompt (strip ANSI before matching)
    let indices: Vec<usize> = lines.iter()
        .enumerate()
        .filter(|(_, line)| prompt::strip_ansi(line.as_bytes()).contains(prompt))
        .map(|(i, _)| i)
        .collect();

    if indices.len() >= 2 {
        // Print from second-to-last prompt up to (but not including) the last prompt
        let start = indices[indices.len() - 2];
        let end = indices[indices.len() - 1];
        for line in &lines[start..end] {
            println!("{}", line);
        }
    } else if indices.len() == 1 {
        // Only one prompt found, print from there onwards
        let start = indices[0];
        for line in &lines[start..] {
            println!("{}", line);
        }
    }

    Ok(())
}

/// Read backwards from the end of a file, doubling the byte window until
/// `predicate` is satisfied or the entire file has been read.
/// Returns at least `min_lines` lines (if available).
fn read_back_until(
    file: &File,
    min_lines: usize,
    predicate: impl Fn(&[String]) -> bool,
) -> Result<Vec<String>> {
    use std::io::{Seek, SeekFrom};

    let mut file = file.try_clone()?;
    let file_size = file.seek(SeekFrom::End(0))?;

    if file_size == 0 {
        return Ok(Vec::new());
    }

    let mut read_bytes = (min_lines as u64) * 80;

    loop {
        let start_pos = if read_bytes >= file_size {
            0
        } else {
            file_size - read_bytes
        };

        file.seek(SeekFrom::Start(start_pos))?;

        let reader = BufReader::new(&file);
        let all_lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;

        if predicate(&all_lines) || start_pos == 0 {
            return Ok(all_lines);
        }

        read_bytes *= 2;
    }
}
