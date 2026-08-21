use std::path::PathBuf;

use console::{measure_text_width, style};

use crate::{
    action::{ActionCatalog, action_fragment_path},
    ui,
};

const DESCRIPTION_MAX_WIDTH: usize = 56;

/// Pads styled text out to `width` using its *visible* width - `measure_text_width`
/// strips ANSI escapes first, so padding after styling (rather than before) still
/// lines columns up correctly.
fn pad(text: &str, width: usize) -> String {
    let visible = measure_text_width(text);
    if visible >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - visible))
    }
}

/// Word-wraps `text` to `width`, hard-breaking any single word that's still
/// too long on its own (so one long identifier can't blow out the column).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate_len = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };

        if candidate_len > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);

        while current.chars().count() > width {
            let head: String = current.chars().take(width).collect();
            let tail: String = current.chars().skip(width).collect();
            lines.push(head);
            current = tail;
        }
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }

    lines
}

fn border(widths: &[usize], left: &str, mid: &str, right: &str) -> String {
    let mut line = left.to_string();
    for (index, width) in widths.iter().enumerate() {
        line.push_str(&"─".repeat(width + 2));
        line.push_str(if index + 1 == widths.len() {
            right
        } else {
            mid
        });
    }
    line
}

fn row(cells: &[String], widths: &[usize]) -> String {
    let bar = ui::muted().apply_to("│").to_string();
    let mut line = bar.clone();
    for (cell, width) in cells.iter().zip(widths) {
        line.push(' ');
        line.push_str(&pad(cell, *width));
        line.push(' ');
        line.push_str(&bar);
    }
    line
}

pub fn plugins(index_path: Option<PathBuf>) -> anyhow::Result<()> {
    let catalog = ActionCatalog::load(index_path.as_deref())?;

    let mut entries = catalog.actions;
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    ui::header("Plugins", "Every action in the catalog, installed or not.");
    println!();

    if entries.is_empty() {
        ui::muted_line("No actions available.");
        return Ok(());
    }

    let name_width = entries
        .iter()
        .map(|entry| entry.name.len())
        .max()
        .unwrap_or(0)
        .max(4);
    let status_width = "installed".len();
    let requires_width = entries
        .iter()
        .map(|entry| entry.dependencies.join(", ").len())
        .max()
        .unwrap_or(0)
        .max("REQUIRES".len());

    let description_lines: Vec<Vec<String>> = entries
        .iter()
        .map(|entry| {
            if entry.description.is_empty() {
                vec!["-".to_string()]
            } else {
                wrap(&entry.description, DESCRIPTION_MAX_WIDTH)
            }
        })
        .collect();
    let description_width = description_lines
        .iter()
        .flatten()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .max("DESCRIPTION".len());

    let widths = [name_width, status_width, requires_width, description_width];

    println!("{}", ui::muted().apply_to(border(&widths, "┌", "┬", "┐")));
    println!(
        "{}",
        row(
            &[
                style("NAME").bold().to_string(),
                style("STATUS").bold().to_string(),
                style("REQUIRES").bold().to_string(),
                style("DESCRIPTION").bold().to_string(),
            ],
            &widths
        )
    );
    println!("{}", ui::muted().apply_to(border(&widths, "├", "┼", "┤")));

    for (entry, description_lines) in entries.iter().zip(&description_lines) {
        let installed = action_fragment_path(&entry.identifier).exists();
        let name = style(&entry.name).bold().to_string();
        let status = if installed {
            ui::success().apply_to("installed").to_string()
        } else {
            ui::muted().apply_to("-").to_string()
        };
        let requires = if entry.dependencies.is_empty() {
            ui::muted().apply_to("-").to_string()
        } else {
            entry.dependencies.join(", ")
        };

        println!(
            "{}",
            row(
                &[name, status, requires, description_lines[0].clone()],
                &widths
            )
        );

        for line in &description_lines[1..] {
            println!(
                "{}",
                row(
                    &[String::new(), String::new(), String::new(), line.clone()],
                    &widths
                )
            );
        }
    }

    println!("{}", ui::muted().apply_to(border(&widths, "└", "┴", "┘")));

    Ok(())
}
