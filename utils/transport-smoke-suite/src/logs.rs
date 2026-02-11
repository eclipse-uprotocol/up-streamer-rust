use anyhow::{anyhow, Context, Result};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

pub async fn wait_for_exact_marker(
    log_path: &Path,
    marker: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;

    loop {
        if has_exact_marker(log_path, marker)? {
            return Ok(());
        }

        if Instant::now() >= deadline {
            let tail = tail_lines(log_path, 25).unwrap_or_default();
            return Err(anyhow!(
                "timed out waiting for marker '{}' in {} (tail: {})",
                marker,
                log_path.display(),
                tail.join(" | ")
            ));
        }

        tokio::time::sleep(poll_interval).await;
    }
}

pub fn count_exact_marker(log_path: &Path, marker: &str) -> Result<usize> {
    if !log_path.exists() {
        return Ok(0);
    }

    let content = fs::read_to_string(log_path)
        .with_context(|| format!("unable to read log file {}", log_path.display()))?;

    Ok(content
        .lines()
        .filter(|line| line.trim_end_matches(['\r', '\n']) == marker)
        .count())
}

pub fn has_exact_marker(log_path: &Path, marker: &str) -> Result<bool> {
    Ok(count_exact_marker(log_path, marker)? > 0)
}

pub fn tail_lines(log_path: &Path, max_lines: usize) -> Result<Vec<String>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(log_path)
        .with_context(|| format!("unable to read log file {}", log_path.display()))?;

    let mut deque = VecDeque::with_capacity(max_lines);
    for line in content.lines() {
        if deque.len() == max_lines {
            deque.pop_front();
        }
        deque.push_back(line.to_string());
    }

    Ok(deque.into_iter().collect())
}
