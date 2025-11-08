use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::{self, Write, BufRead, BufReader};

use crate::session;

/// Write to a session's stdin pipe
pub fn write_session(session_name: &str, args: &[String]) -> Result<()> {
    let stdin_path = session::stdin_path(session_name)?;

    if !stdin_path.exists() {
        anyhow::bail!("no stdin at {} (is the session running?)", stdin_path.display());
    }

    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(&stdin_path)
        .with_context(|| format!("failed to open {}", stdin_path.display()))?;

    if !args.is_empty() {
        // Write args as a single line
        let line = args.join(" ");
        writeln!(file, "{}", line)
            .with_context(|| "failed to write to session stdin")?;
    } else {
        // Read from stdin and forward to the pipe
        let stdin = io::stdin();
        let reader = BufReader::new(stdin);

        for line in reader.lines() {
            let line = line.with_context(|| "failed to read from stdin")?;
            writeln!(file, "{}", line)
                .with_context(|| "failed to write to session stdin")?;
        }
    }

    Ok(())
}
