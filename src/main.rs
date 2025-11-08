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
        // via → list sessions
        session::list_sessions()?;
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
  via                                   # list sessions
  via help                              # this help
  via <session> help                    # help for a specific session name
  via <session> run -- <cmd> ...        # start a new named (interactive) session running <cmd>
  via <session> 'PROMPT>' [line...]     # write input and read output in one command

low-level usage:
  via <session> write [line...]         # write (reads stdin if none)
  via <session> tail -n N               # tail last N lines
  via <session> tail -f [-n N]          # follow output in real-time
  via <session> tail --since 'PROMPT>'  # tail since last PROMPT>
  via <session> tail --delim 'PROMPT>'  # last stanza since PROMPT>
  via <session> path                    # show session path"#);
}

fn show_session_usage(session: &str) {
    println!(r#"usage for '{session}':
  via {session} help                    # help for a specific session session
  via {session} 'PROMPT>' [line...]     # write input and read output in one command

low-level usage:
  via {session} write [line...]         # write (reads stdin if none)
  via {session} tail -n N               # tail last N lines
  via {session} tail -f [-n N]          # follow output in real-time
  via {session} tail --since 'PROMPT>'  # tail since last PROMPT>
  via {session} tail --delim 'PROMPT>'  # last stanza since PROMPT>
  via {session} path                    # show session path"#);
}

fn cmd_run(session: &str, args: &[String]) -> Result<()> {
    // Expect args to start with "--"
    if args.is_empty() || args[0] != "--" {
        anyhow::bail!("usage: via {} run -- <command> [args...]", session);
    }

    if args.len() < 2 {
        anyhow::bail!("usage: via {} run -- <command> [args...]", session);
    }

    // Get session directory and create it
    let dir = session::session_path(session)?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create directory {}", dir.display()))?;

    let stdin_path = dir.join("stdin");
    let stdout_path = dir.join("stdout");

    // Print session info
    println!("[via] dir: {}", dir.display());
    println!("[via] stdin: {}", stdin_path.display());
    println!("[via] stdout: {}", stdout_path.display());
    println!("[via] launching: {}", args[1..].join(" "));

    // Build teetty command
    let mut cmd = std::process::Command::new("teetty");
    cmd.arg("-i").arg(&stdin_path)
       .arg("-o").arg(&stdout_path)
       .arg("--truncate")
       .arg("--");

    // Add the command and args
    for arg in &args[1..] {
        cmd.arg(arg);
    }

    // Set up cleanup handler for Ctrl-C
    let dir_for_cleanup = dir.clone();
    ctrlc::set_handler(move || {
        // Cleanup is handled by removing directory
        let _ = std::fs::remove_dir_all(&dir_for_cleanup);
        std::process::exit(130); // Standard exit code for SIGINT
    }).ok();

    // Run teetty (foreground, interactive)
    let status = cmd.status()
        .with_context(|| "failed to execute teetty (is it installed?)")?;

    // Cleanup directory after teetty exits
    std::fs::remove_dir_all(&dir).ok();

    // Exit with same code as teetty
    if let Some(code) = status.code() {
        std::process::exit(code);
    } else {
        // Killed by signal
        std::process::exit(1);
    }
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
    // 1. Check that the prompt is at the end of the output
    prompt::check_prompt_ready(session, prompt)?;

    // 2. Write input (from args or stdin)
    fifo::write_session(session, args)?;

    // 3. Wait for the command to execute and new prompt to appear
    prompt::wait_for_prompt(session, prompt)?;

    // 4. Tail output since the delimiter
    tail::tail_delim(session, prompt)
}
