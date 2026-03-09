use anyhow::{Context, Result};
use std::env;
use std::io::Write;
use std::process::exit;

mod session;
mod fifo;
mod tail;
mod prompt;

fn main() {
    exit(match run() {
        Ok(code) => code,
        Err(err) => {
            writeln!(std::io::stderr(), "via: {}", err).ok();
            1
        }
    })
}

fn run() -> Result<i32> {
    let mut args: Vec<String> = env::args().collect();

    // Remove program name
    args.remove(0);

    // Check for global flags first
    if !args.is_empty() {
        match args[0].as_str() {
            "--help" | "-h" => {
                show_usage_global();
                return Ok(0);
            }
            "--version" => {
                eprintln!("via {}", env!("CARGO_PKG_VERSION"));
                return Ok(0);
            }
            _ => {}
        }
    }

    if args.is_empty() {
        // via → list sessions (table format)
        session::list_sessions(false)?;
        return Ok(0);
    }

    // Check for --simple flag before other processing
    if args[0] == "--simple" {
        session::list_sessions(true)?;
        return Ok(0);
    }

    let first_arg = &args[0];

    if first_arg == "help" {
        // via help → show global usage
        show_usage_global();
        return Ok(0);
    }

    // First arg is the session name
    let session_name = first_arg.clone();

    if args.len() < 2 {
        anyhow::bail!("missing subcommand for '{}' (try: via {} help)", session_name, session_name);
    }

    let subcmd = &args[1];
    let remaining_args = &args[2..];

    match subcmd.as_str() {
        "help" => {
            show_session_usage(&session_name);
            Ok(0)
        }
        "run" => {
            cmd_run(&session_name, remaining_args)?;
            Ok(0)
        }
        "wait" => {
            cmd_wait(&session_name, remaining_args)?;
            Ok(0)
        }
        "write" => {
            cmd_write(&session_name, remaining_args)?;
            Ok(0)
        }
        "tail" => {
            cmd_tail(&session_name, remaining_args)?;
            Ok(0)
        }
        "path" => {
            cmd_path(&session_name)?;
            Ok(0)
        }
        s if s.ends_with('>') => {
            // Shorthand: via <session> 'PROMPT>' [line...]
            cmd_prompt_shorthand(&session_name, subcmd, remaining_args)?;
            Ok(0)
        }
        _ => {
            anyhow::bail!("unknown subcommand for '{}': {} (try: via {} help)",
                         session_name, subcmd, session_name);
        }
    }
}

fn show_usage_global() {
    println!(r#"usage:
  via [--simple]                                          # list sessions (tabular format by default)
  via help                                                # this help
  via <session> help                                      # help for a specific session name
  via <session> run -- <cmd> ...                          # start a new session running <cmd> (blocks)
  via <session> wait 'PROMPT>' [--timeout N]              # wait silently for prompt to appear
  via <session> 'PROMPT>' [--timeout N] [line]            # write input and stream output until prompt

low-level usage:
  via <session> write [line...]                           # write (reads stdin if none)
  via <session> tail -n N                                 # tail last N lines
  via <session> tail -f [-n N]                            # follow output in real-time
  via <session> tail --since 'PROMPT>'                    # tail since last PROMPT>
  via <session> tail --delim 'PROMPT>'                    # last stanza since PROMPT>
  via <session> tail --until 'PROMPT>' [--timeout N]      # stream output until PROMPT> appears
  via <session> tail --since 'P>' --until 'P>' [--timeout N]  # stream from last P> until next
  via <session> path                                      # show session path"#);
}

fn show_session_usage(session: &str) {
    println!(r#"usage for '{session}':
  via {session} help                                      # help for a specific session
  via {session} wait 'PROMPT>' [--timeout N]              # wait silently for prompt to appear
  via {session} 'PROMPT>' [--timeout N] [line]            # write input and stream output until prompt

low-level usage:
  via {session} write [line...]                           # write (reads stdin if none)
  via {session} tail -n N                                 # tail last N lines
  via {session} tail -f [-n N]                            # follow output in real-time
  via {session} tail --since 'PROMPT>'                    # tail since last PROMPT>
  via {session} tail --delim 'PROMPT>'                    # last stanza since PROMPT>
  via {session} tail --until 'PROMPT>' [--timeout N]      # stream output until PROMPT> appears
  via {session} path                                      # show session path"#);
}

fn cmd_run(session: &str, args: &[String]) -> Result<()> {
    // Expect "--" separator and at least one command arg
    let separator_pos = args.iter().position(|a| a == "--");
    match separator_pos {
        None => anyhow::bail!("usage: via {} run -- <command> [args...]", session),
        Some(pos) if pos + 1 >= args.len() => {
            anyhow::bail!("usage: via {} run -- <command> [args...]", session)
        }
        _ => {}
    }
    let separator_pos = separator_pos.unwrap();
    let cmd_args = &args[separator_pos + 1..];

    // Get session directory and create it
    let dir = session::session_path(session)?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create directory {}", dir.display()))?;

    // Write metadata files
    let command = cmd_args.join(" ");
    std::fs::write(dir.join("command"), &command)
        .with_context(|| "failed to write command metadata")?;

    if let Ok(cwd) = env::current_dir() {
        std::fs::write(dir.join("cwd"), cwd.to_string_lossy().as_bytes())
            .with_context(|| "failed to write cwd metadata")?;
    }

    let stdin_path = dir.join("stdin");
    let stdout_path = dir.join("stdout");

    // Print session info
    eprintln!("[via] dir: {}", dir.display());
    eprintln!("[via] stdin: {}", stdin_path.display());
    eprintln!("[via] stdout: {}", stdout_path.display());
    eprintln!("[via] launching: {}", command);

    // Build teetty command
    let mut cmd = std::process::Command::new("teetty");
    cmd.arg("-i").arg(&stdin_path)
       .arg("-o").arg(&stdout_path)
       .arg("--truncate")
       .arg("--");

    // Add the command and args
    for arg in cmd_args {
        cmd.arg(arg);
    }

    // Set up cleanup handler for Ctrl-C
    let dir_for_cleanup = dir.clone();
    ctrlc::set_handler(move || {
        let _ = std::fs::remove_dir_all(&dir_for_cleanup);
        std::process::exit(130);
    }).ok();

    // Run teetty in the foreground — blocks until the subprocess exits.
    // This keeps teetty as a child of `via`, so killing `via` also
    // terminates teetty.
    let status = cmd.status()
        .with_context(|| "failed to execute teetty (is it installed?)")?;

    // Cleanup directory after teetty exits
    std::fs::remove_dir_all(&dir).ok();

    if let Some(code) = status.code() {
        std::process::exit(code);
    } else {
        std::process::exit(1);
    }
}

/// Wait for a prompt to appear in an already-running session's output.
/// Thin wrapper around tail --until with suppressed output.
fn cmd_wait(session: &str, args: &[String]) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!("usage: via {} wait 'PROMPT>' [--timeout N]", session);
    }

    let prompt = &args[0];
    let remaining = &args[1..];

    // Parse optional --timeout
    let mut timeout = tail::DEFAULT_TIMEOUT;
    let mut i = 0;
    while i < remaining.len() {
        match remaining[i].as_str() {
            "--timeout" => {
                if i + 1 >= remaining.len() {
                    anyhow::bail!("--timeout requires a number");
                }
                timeout = remaining[i + 1].parse()
                    .with_context(|| format!("invalid timeout: {}", remaining[i + 1]))?;
                i += 2;
            }
            _ => {
                anyhow::bail!("usage: via {} wait 'PROMPT>' [--timeout N]", session);
            }
        }
    }

    tail::follow_until(session, prompt, timeout, 0, &mut std::io::sink())?;
    eprintln!("[via] ready (prompt detected)");
    Ok(())
}

fn cmd_write(session: &str, args: &[String]) -> Result<()> {
    fifo::write_session(session, args)
}

fn cmd_tail(session: &str, args: &[String]) -> Result<()> {
    tail::tail_session(session, args)
}

fn cmd_path(session: &str) -> Result<()> {
    let path = session::session_path(session)?;
    println!("{}", path.display());
    Ok(())
}

fn cmd_prompt_shorthand(session: &str, prompt: &str, args: &[String]) -> Result<()> {
    // Parse optional --timeout flag from the args
    let (input_args, timeout) = parse_timeout_flag(args);

    // 1. Check that the prompt is at the end of the output
    prompt::check_prompt_ready(session, prompt)?;

    // 2. Record current file position before writing
    let stdout_path = session::stdout_path(session)?;
    let pos = {
        use std::io::{Seek, SeekFrom};
        let mut f = std::fs::File::open(&stdout_path)?;
        f.seek(SeekFrom::End(0))?
    };

    // 3. Write input (from args or stdin)
    fifo::write_session(session, &input_args)?;

    // 4. Stream output until the next prompt appears
    tail::follow_until(session, prompt, timeout, pos, &mut std::io::stdout())
}

/// Extract --timeout N from an argument list.
/// Returns the remaining args and the timeout value.
fn parse_timeout_flag(args: &[String]) -> (Vec<String>, f64) {
    let mut remaining = args.to_vec();
    let mut timeout = tail::DEFAULT_TIMEOUT;

    // Look for --timeout anywhere in the args
    if let Some(idx) = remaining.iter().position(|a| a == "--timeout") {
        if idx + 1 < remaining.len() {
            if let Ok(t) = remaining[idx + 1].parse::<f64>() {
                timeout = t;
            }
            remaining.remove(idx + 1);
        }
        remaining.remove(idx);
    }

    (remaining, timeout)
}
