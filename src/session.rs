use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Get the base directory for sessions, with fallback logic
pub fn base_dir() -> Result<PathBuf> {
    let default_base = PathBuf::from("/run/via");

    // Check REPLS_DIR environment variable first
    let base = if let Ok(repls_dir) = env::var("REPLS_DIR") {
        PathBuf::from(repls_dir)
    } else {
        default_base
    };

    // Try to create the directory if it doesn't exist
    if !base.exists() {
        if fs::create_dir_all(&base).is_ok() {
            return Ok(base);
        }

        // Fallback to XDG_RUNTIME_DIR or /tmp
        let fallback = if let Ok(xdg) = env::var("XDG_RUNTIME_DIR") {
            PathBuf::from(xdg).join("via")
        } else {
            PathBuf::from("/tmp/via")
        };

        fs::create_dir_all(&fallback)
            .with_context(|| format!("can't create {}", fallback.display()))?;

        return Ok(fallback);
    }

    Ok(base)
}

/// List all sessions (directories in base dir)
pub fn list_sessions() -> Result<()> {
    let base = base_dir()?;

    let mut sessions = Vec::new();

    if let Ok(entries) = fs::read_dir(&base) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        sessions.push(name.to_string());
                    }
                }
            }
        }
    }

    sessions.sort();

    for session in sessions {
        println!("{}", session);
    }

    Ok(())
}

/// Get the path for a specific session
pub fn session_path(session: &str) -> Result<PathBuf> {
    let base = base_dir()?;
    Ok(base.join(session))
}

/// Get the stdin pipe path for a session
pub fn stdin_path(session: &str) -> Result<PathBuf> {
    Ok(session_path(session)?.join("stdin"))
}

/// Get the stdout file path for a session
pub fn stdout_path(session: &str) -> Result<PathBuf> {
    Ok(session_path(session)?.join("stdout"))
}

/// Check if a session exists (has both stdin and stdout)
#[allow(dead_code)]
pub fn session_exists(session: &str) -> Result<bool> {
    let stdin = stdin_path(session)?;
    let stdout = stdout_path(session)?;
    Ok(stdin.exists() && stdout.exists())
}
