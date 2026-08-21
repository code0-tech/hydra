use std::{
    collections::HashMap,
    fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
};

use crate::ui;

use super::Runner;

pub struct DockerComposeRunner {
    env_file: PathBuf,
    compose_file: PathBuf,
    override_compose_file: PathBuf,
    actions_dir: PathBuf,
}

impl Default for DockerComposeRunner {
    fn default() -> Self {
        Self {
            env_file: ".codezero/.env".into(),
            compose_file: ".codezero/docker-compose.yml".into(),
            override_compose_file: ".codezero/docker-compose.override.yml".into(),
            actions_dir: ".codezero/actions".into(),
        }
    }
}

/// The main compose file, then the override file if the bundle provided one
/// (e.g. mounting `.codezero/service.configuration.json` into aquila - see
/// `bundle/manifest.json`'s optional `docker-compose.override.yml` template),
/// followed by every installed action's compose fragment (sorted for a
/// stable, reproducible `-f` order).
fn collect_compose_files(
    compose_file: &Path,
    override_compose_file: &Path,
    actions_dir: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = vec![compose_file.to_path_buf()];

    if override_compose_file.is_file() {
        files.push(override_compose_file.to_path_buf());
    }

    if actions_dir.is_dir() {
        let mut extra: Vec<PathBuf> = fs::read_dir(actions_dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "yml"))
            .collect();
        extra.sort();
        files.extend(extra);
    }

    Ok(files)
}

/// Parses a size like "13.53MB", "340kB", or "1.2GB" into bytes. Docker's own
/// progress output uses decimal (1000-based) units.
fn parse_size(token: &str) -> Option<u64> {
    let split_at = token.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, unit) = token.split_at(split_at);
    let value: f64 = number.parse().ok()?;
    let multiplier: f64 = match unit {
        "B" => 1.0,
        "kB" | "KB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        "TB" => 1_000_000_000_000.0,
        _ => return None,
    };
    Some((value * multiplier) as u64)
}

/// Extracts a `(key, current, total)` progress reading from one line of
/// `docker compose pull --progress=plain` output, e.g.
/// `gls-action 5eb901aaf2ae Downloading [====>   ]  13.53MB/28.51MB`. The key
/// is everything before the literal "Downloading" (service/layer id), which
/// stays stable across repeated updates for the same layer — unlike the
/// progress bar or sizes, which change every line — so each layer's latest
/// reading can be tracked independently instead of double-counted.
fn parse_progress_line(line: &str) -> Option<(String, u64, u64)> {
    let (key, rest) = line.split_once("Downloading")?;

    for word in rest.split_whitespace() {
        if let Some((a, b)) = word.split_once('/')
            && let (Some(current), Some(total)) = (parse_size(a), parse_size(b))
        {
            return Some((key.trim().to_string(), current, total));
        }
    }

    None
}

impl DockerComposeRunner {
    fn docker_compose(&self, args: &[&str]) -> anyhow::Result<Output> {
        let compose_files = collect_compose_files(
            &self.compose_file,
            &self.override_compose_file,
            &self.actions_dir,
        )?;

        let mut command = Command::new("docker");
        command.args(["compose", "--env-file"]).arg(&self.env_file);
        for file in &compose_files {
            command.arg("-f").arg(file);
        }

        let output = command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            anyhow::bail!(
                "CodeZero command failed\n{}{}",
                stdout.trim(),
                stderr.trim()
            );
        }

        Ok(output)
    }

    /// Runs a compose command with the terminal wired straight through
    /// (rather than captured), so interactive/streaming output like
    /// `ps`'s table formatting or `logs --follow` behaves natively.
    fn docker_compose_interactive(&self, args: &[&str]) -> anyhow::Result<()> {
        let compose_files = collect_compose_files(
            &self.compose_file,
            &self.override_compose_file,
            &self.actions_dir,
        )?;

        let mut command = Command::new("docker");
        command.args(["compose", "--env-file"]).arg(&self.env_file);
        for file in &compose_files {
            command.arg("-f").arg(file);
        }

        let status = command.args(args).status()?;

        if !status.success() {
            anyhow::bail!("CodeZero command failed");
        }

        Ok(())
    }

    fn services(&self) -> anyhow::Result<Vec<String>> {
        let output = self.docker_compose(&["config", "--services"])?;
        let stdout = String::from_utf8(output.stdout)?;
        let services = stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        Ok(services)
    }

    /// Pulls every service's image at once, tracking real download progress
    /// by parsing `docker compose pull`'s output as it streams, rather than
    /// jumping in whole-service increments. `--progress` isn't passed since
    /// it's not supported by `pull` on every Compose version — piping
    /// stdout/stderr already makes Compose fall back to non-interactive
    /// (plain-style) output on its own, since it detects there's no TTY.
    fn pull_with_progress(&self, label: &str, services: &[String]) -> anyhow::Result<()> {
        let compose_files = collect_compose_files(
            &self.compose_file,
            &self.override_compose_file,
            &self.actions_dir,
        )?;

        let mut command = Command::new("docker");
        command.args(["compose", "--env-file"]).arg(&self.env_file);
        for file in &compose_files {
            command.arg("-f").arg(file);
        }
        command.arg("pull").args(services);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("stdout should be piped");
        let stderr = child.stderr.take().expect("stderr should be piped");

        let (tx, rx) = mpsc::channel::<String>();
        let tx_stderr = tx.clone();
        let stdout_thread = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        let stderr_thread = thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx_stderr.send(line);
            }
        });

        let mut layers: HashMap<String, (u64, u64)> = HashMap::new();
        let mut output = String::new();
        self.print_progress_bytes(label, 0, 1);

        for line in rx.iter() {
            match parse_progress_line(&line) {
                Some((key, current, total)) if total > 0 => {
                    layers.insert(key, (current, total));
                    let (sum_current, sum_total) = layers
                        .values()
                        .fold((0u64, 0u64), |acc, &(c, t)| (acc.0 + c, acc.1 + t));
                    self.print_progress_bytes(label, sum_current, sum_total);
                }
                // Keep only non-progress lines for the error message below,
                // so a failure shows the actual reason instead of megabytes
                // of interleaved download-progress noise.
                _ => {
                    output.push_str(&line);
                    output.push('\n');
                }
            }
        }

        let _ = stdout_thread.join();
        let _ = stderr_thread.join();
        let status = child.wait()?;

        if !status.success() {
            print!("\r\x1b[2K");
            anyhow::bail!("CodeZero command failed\n{}", output.trim());
        }

        self.finish_progress(label);
        Ok(())
    }

    fn start_services(&self, services: &[String]) -> anyhow::Result<()> {
        let label = "start CodeZero";

        if services.is_empty() {
            self.print_progress(label, 0, 1);
            self.docker_compose(&["up", "-d"])?;
            self.print_progress(label, 1, 1);
            self.finish_progress(label);
            return Ok(());
        }

        for (index, service) in services.iter().enumerate() {
            self.print_progress(label, index, services.len());
            self.docker_compose(&["up", "-d", service])?;
            self.print_progress(label, index + 1, services.len());
        }

        self.finish_progress(label);
        Ok(())
    }

    fn print_progress(&self, label: &str, current: usize, total: usize) {
        let total = total.max(1);
        self.print_progress_fraction(label, current as f64 / total as f64);
    }

    fn print_progress_bytes(&self, label: &str, current: u64, total: u64) {
        let total = total.max(1);
        self.print_progress_fraction(label, current as f64 / total as f64);
    }

    fn print_progress_fraction(&self, label: &str, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
        let percent = (fraction * 100.0) as u64;
        let width = 28;
        let filled = (width as f64 * fraction).round() as usize;
        let empty = width - filled;
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

        print!(
            "\r\x1b[2K  {}{}{}  {:>3}%  {}",
            ui::muted().apply_to("▐"),
            ui::accent().apply_to(bar),
            ui::muted().apply_to("▌"),
            percent,
            ui::muted().apply_to(label)
        );

        let _ = io::stdout().flush();
    }

    fn finish_progress(&self, label: &str) {
        print!("\r\x1b[2K");
        ui::success_line(label);
    }
}

impl Runner for DockerComposeRunner {
    fn start(&self) -> anyhow::Result<()> {
        ui::header(
            "CodeZero startup",
            "Preparing CodeZero components. Detailed backend output is hidden.",
        );
        println!();

        let services = self.services()?;

        self.regenerate_configs()?;

        self.pull_with_progress("download components", &services)?;
        self.start_services(&services)?;

        println!();
        ui::success_line("CodeZero is starting.");
        Ok(())
    }

    fn stop(&self, wipe_volumes: bool) -> anyhow::Result<()> {
        ui::header("CodeZero shutdown", "Stopping CodeZero components.");
        println!();

        let label = "stop CodeZero";
        self.print_progress(label, 0, 1);
        let mut args = vec!["down"];
        if wipe_volumes {
            args.push("-v");
        }
        self.docker_compose(&args)?;
        self.print_progress(label, 1, 1);
        self.finish_progress(label);

        println!();
        ui::success_line("CodeZero stopped.");
        Ok(())
    }

    fn regenerate_configs(&self) -> anyhow::Result<()> {
        // config-generator has `restart: "no"` and runs once to populate the
        // shared generated-configs volume. If it already ran to completion in
        // an earlier `start` (e.g. against an older .env, before a config
        // change), Compose considers it done and won't rerun it on a plain
        // `up -d` — silently reusing whatever stale config it wrote the first
        // time. Force it stale so the next `up`/`start_service` reruns it
        // fresh (Compose reruns anything depending on it, like aquila, only
        // after it completes again).
        let _ = self.docker_compose(&["rm", "-f", "-s", "config-generator"]);
        Ok(())
    }

    fn restart(&self, service: &str) -> anyhow::Result<()> {
        self.docker_compose(&["restart", service])?;
        Ok(())
    }

    fn start_service(&self, service: &str) -> anyhow::Result<()> {
        self.pull_with_progress(&format!("download {service}"), &[service.to_string()])?;

        let label = format!("start {service}");
        self.print_progress(&label, 0, 1);
        self.docker_compose(&["up", "-d", service])?;
        self.print_progress(&label, 1, 1);
        self.finish_progress(&label);

        Ok(())
    }

    fn stop_service(&self, service: &str) -> anyhow::Result<()> {
        self.docker_compose(&["rm", "-sf", service])?;
        Ok(())
    }

    fn status(&self) -> anyhow::Result<()> {
        self.docker_compose_interactive(&["ps"])
    }

    fn logs(&self, service: Option<&str>, follow: bool, tail: Option<u32>) -> anyhow::Result<()> {
        let mut args = vec!["logs".to_string()];
        if follow {
            args.push("--follow".to_string());
        }
        if let Some(tail) = tail {
            args.push(format!("--tail={tail}"));
        }
        if let Some(service) = service {
            args.push(service.to_string());
        }

        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.docker_compose_interactive(&args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_sorted_action_fragments_after_the_main_compose_file() {
        let dir =
            std::env::temp_dir().join(format!("hydra-compose-files-test-{}", std::process::id()));
        let actions_dir = dir.join("actions");
        fs::create_dir_all(&actions_dir).unwrap();
        fs::write(actions_dir.join("shopware-action.yml"), "").unwrap();
        fs::write(actions_dir.join("gls-action.yml"), "").unwrap();
        fs::write(actions_dir.join("ignored.txt"), "").unwrap();

        let compose_file = dir.join("docker-compose.yml");
        let override_compose_file = dir.join("docker-compose.override.yml");
        let files =
            collect_compose_files(&compose_file, &override_compose_file, &actions_dir).unwrap();

        assert_eq!(
            files,
            vec![
                compose_file,
                actions_dir.join("gls-action.yml"),
                actions_dir.join("shopware-action.yml"),
            ]
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_the_main_compose_file_when_no_actions_dir() {
        let compose_file = PathBuf::from(".codezero/docker-compose.yml");
        let override_compose_file = PathBuf::from(".codezero/does-not-exist.yml");
        let actions_dir = PathBuf::from(".codezero/does-not-exist");

        let files =
            collect_compose_files(&compose_file, &override_compose_file, &actions_dir).unwrap();

        assert_eq!(files, vec![compose_file]);
    }

    #[test]
    fn includes_the_override_file_when_it_exists_right_after_the_main_compose_file() {
        let dir = std::env::temp_dir().join(format!(
            "hydra-compose-override-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();

        let compose_file = dir.join("docker-compose.yml");
        let override_compose_file = dir.join("docker-compose.override.yml");
        fs::write(&override_compose_file, "").unwrap();
        let actions_dir = dir.join("does-not-exist");

        let files =
            collect_compose_files(&compose_file, &override_compose_file, &actions_dir).unwrap();

        assert_eq!(files, vec![compose_file, override_compose_file]);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parses_decimal_size_units() {
        assert_eq!(parse_size("52MB"), Some(52_000_000));
        assert_eq!(parse_size("13.53MB"), Some(13_530_000));
        assert_eq!(parse_size("340kB"), Some(340_000));
        assert_eq!(parse_size("1.2GB"), Some(1_200_000_000));
        assert_eq!(parse_size("not-a-size"), None);
    }

    #[test]
    fn parses_a_downloading_progress_line() {
        let line = "gls-action 5eb901aaf2ae Downloading [====>       ]  13.53MB/28.51MB";
        let (key, current, total) = parse_progress_line(line).expect("line should parse");

        assert_eq!(key, "gls-action 5eb901aaf2ae");
        assert_eq!(current, 13_530_000);
        assert_eq!(total, 28_510_000);
    }

    #[test]
    fn ignores_lines_without_a_downloading_size_pair() {
        assert_eq!(parse_progress_line("gls-action Pull complete"), None);
        assert_eq!(parse_progress_line("gls-action Waiting"), None);
    }

    #[test]
    fn tracks_multiple_concurrent_layers_by_stable_key_not_by_bar_art() {
        let mut layers: HashMap<String, (u64, u64)> = HashMap::new();

        for line in [
            "img a Downloading [>         ]  1MB/10MB",
            "img b Downloading [==>       ]  2MB/20MB",
            "img a Downloading [===>      ]  3MB/10MB",
        ] {
            let (key, current, total) = parse_progress_line(line).unwrap();
            layers.insert(key, (current, total));
        }

        assert_eq!(layers.len(), 2);
        assert_eq!(layers["img a"], (3_000_000, 10_000_000));
        assert_eq!(layers["img b"], (2_000_000, 20_000_000));
    }
}
