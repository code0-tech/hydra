use std::{
    io::{self, Write},
    path::PathBuf,
    process::{Command, Output, Stdio},
};

use console::style;

use super::Runner;

pub struct DockerComposeRunner {
    env_file: PathBuf,
    compose_file: PathBuf,
}

impl Default for DockerComposeRunner {
    fn default() -> Self {
        Self {
            env_file: ".codezero/.env".into(),
            compose_file: ".codezero/docker-compose.yml".into(),
        }
    }
}

impl DockerComposeRunner {
    fn docker_compose(&self, args: &[&str]) -> anyhow::Result<Output> {
        let output = Command::new("docker")
            .args(["compose", "--env-file"])
            .arg(&self.env_file)
            .arg("-f")
            .arg(&self.compose_file)
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

    fn run_for_each_service(
        &self,
        label: &str,
        command: &str,
        services: &[String],
    ) -> anyhow::Result<()> {
        if services.is_empty() {
            self.print_progress(label, 0, 1);
            self.docker_compose(&[command])?;
            self.print_progress(label, 1, 1);
            println!();
            return Ok(());
        }

        for (index, service) in services.iter().enumerate() {
            self.print_progress(label, index, services.len());
            self.docker_compose(&[command, "--quiet", service])?;
            self.print_progress(label, index + 1, services.len());
        }

        println!();
        Ok(())
    }

    fn start_services(&self, services: &[String]) -> anyhow::Result<()> {
        if services.is_empty() {
            self.print_progress("start CodeZero", 0, 1);
            self.docker_compose(&["up", "-d"])?;
            self.print_progress("start CodeZero", 1, 1);
            println!();
            return Ok(());
        }

        for (index, service) in services.iter().enumerate() {
            self.print_progress("start CodeZero", index, services.len());
            self.docker_compose(&["up", "-d", service])?;
            self.print_progress("start CodeZero", index + 1, services.len());
        }

        println!();
        Ok(())
    }

    fn print_progress(&self, label: &str, current: usize, total: usize) {
        let total = total.max(1);
        let percent = current * 100 / total;
        let width = 28;
        let filled = width * current / total;
        let empty = width - filled;
        let bar = format!("{}{}", "#".repeat(filled), "-".repeat(empty));

        print!(
            "\r\x1b[2K  {} {:>3}% {}",
            style(format!("[{bar}]")).cyan(),
            percent,
            style(label).bold()
        );

        let _ = io::stdout().flush();
    }
}

impl Runner for DockerComposeRunner {
    fn start(&self) -> anyhow::Result<()> {
        println!();
        println!("{}", style("CodeZero startup").cyan().bold());
        println!(
            "{}",
            style("Preparing CodeZero components. Detailed backend output is hidden.").dim()
        );

        let services = self.services()?;

        self.run_for_each_service("download components", "pull", &services)?;
        self.start_services(&services)?;

        println!("{}", style("CodeZero is starting.").green().bold());
        Ok(())
    }

    fn stop(&self) -> anyhow::Result<()> {
        println!();
        self.print_progress("stop CodeZero", 0, 1);
        self.docker_compose(&["down", "-v"])?;
        self.print_progress("stop CodeZero", 1, 1);
        println!();

        println!("{}", style("CodeZero stopped.").green().bold());
        Ok(())
    }
}
