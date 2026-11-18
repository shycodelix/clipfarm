use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Small helpers shared across the engine. No hidden state, no globals.
pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    value.max(low).min(high)
}

pub fn slugify(input: &str, max_len: usize) -> String {
    let mut slug = String::with_capacity(input.len());
    let mut last_dash = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= max_len {
            break;
        }
    }
    slug.trim_matches('-').to_string()
}

pub fn human_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Find an executable on PATH without shelling out.
pub fn which(program: &str) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if candidate.is_absolute() && candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|path| path.is_file())
}

/// Run a command with a timeout and capture its output. Used for cheap probes
/// (encoder lists, version banners), never for long renders.
pub fn run_with_timeout(command: &mut Command, timeout_ms: u128) -> Result<std::process::Output> {
    let label = format!("{:?}", command);
    let child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("Could not start {label}"))?;
    let started = Instant::now();
    // std has no wait_timeout; poll instead of pulling in a dependency.
    let mut child = child;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None => {
                if started.elapsed().as_millis() > timeout_ms {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(anyhow!("Command timed out after {timeout_ms} ms: {label}"));
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
        }
    }
    child
        .wait_with_output()
        .context("Could not read command output")
}

/// Parse FFmpeg's `-progress` key=value stream for the current position.
pub fn parse_progress_seconds(line: &str) -> Option<f64> {
    let line = line.trim();
    if let Some(value) = line.strip_prefix("out_time_us=") {
        return value.parse::<f64>().ok().map(|us| us / 1_000_000.0);
    }
    if let Some(value) = line.strip_prefix("out_time_ms=") {
        // Older FFmpeg reports milliseconds under this key.
        return value.parse::<f64>().ok().map(|ms| ms / 1_000.0);
    }
    if let Some(value) = line.strip_prefix("out_time=") {
        let mut parts = value.split(':');
        let h: f64 = parts.next()?.parse().ok()?;
        let m: f64 = parts.next()?.parse().ok()?;
        let s: f64 = parts.next()?.parse().ok()?;
        return Some(h * 3600.0 + m * 60.0 + s);
    }
    None
}

/// Atomically replace a text file so a crash cannot leave a half-written project.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}", parent.display()))?;
    }
    let temporary = path.with_extension("tmp-write");
    std::fs::write(&temporary, contents)
        .with_context(|| format!("Could not write {}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("Could not replace {}", path.display()))?;
    Ok(())
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).with_context(|| format!("Could not create {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_keeps_it_readable() {
        assert_eq!(slugify("Mara Chen — Interview!", 40), "mara-chen-interview");
        assert_eq!(slugify("***", 10), "");
    }

    #[test]
    fn progress_parser_handles_both_units() {
        assert_eq!(parse_progress_seconds("out_time_us=1500000"), Some(1.5));
        assert_eq!(parse_progress_seconds("out_time_ms=2500"), Some(2.5));
        assert_eq!(
            parse_progress_seconds("out_time=00:00:03.500000"),
            Some(3.5)
        );
    }

    #[test]
    fn durations_are_human() {
        assert_eq!(human_duration(59.4), "0:59");
        assert_eq!(human_duration(3661.0), "1:01:01");
    }
}
