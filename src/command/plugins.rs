use std::path::PathBuf;

use console::{measure_text_width, style};

use crate::{
    action::{
        ActionCatalog, SERVICE_CONFIGURATION_PATH, ServiceConfiguration, action_fragment_path,
    },
    ui,
};

const DESCRIPTION_MAX_WIDTH: usize = 56;

/// How an action shows up in the `plugin ls` STATUS column.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    /// A catalog action with a compose fragment under `.codezero/actions/`
    /// (i.e. added via `plugin install`).
    Installed,
    /// An identifier `plugin register`ed with Aquila but not in the catalog -
    /// a locally-developed action pointed at the runtime by hand.
    Registered,
    /// A catalog action that isn't installed.
    Available,
}

impl Status {
    /// The plain (unstyled) label, used for column-width sizing.
    fn label(self) -> &'static str {
        match self {
            Status::Installed => "installed",
            Status::Registered => "registered",
            Status::Available => "-",
        }
    }

    /// The styled cell as printed. `pad` measures visible width, so styling
    /// here (after sizing off `label`) still lines the column up.
    fn cell(self) -> String {
        match self {
            Status::Installed => ui::success().apply_to("installed").to_string(),
            Status::Registered => ui::secondary().apply_to("registered").to_string(),
            Status::Available => ui::muted().apply_to("-").to_string(),
        }
    }
}

/// One rendered table row — a catalog action or a locally-registered one,
/// flattened so width-sizing and printing don't care which it came from.
struct DisplayRow {
    name: String,
    status: Status,
    /// Comma-joined dependencies, empty when there are none.
    requires: String,
    /// Catalog description, empty when there is none (or for registered actions,
    /// which have no catalog entry).
    description: String,
}

/// The identifiers configured locally (via `plugin register`) that aren't in
/// the catalog — the actions `plugin ls` would otherwise never mention.
/// Installed catalog actions and base-stack ones (rest/cron) are in the catalog
/// too, so they're excluded here and shown as normal catalog rows instead.
fn registered_only(catalog: &ActionCatalog, config: &ServiceConfiguration) -> Vec<String> {
    let mut ids: Vec<String> = config
        .actions
        .iter()
        .filter(|action| catalog.find(&action.identifier).is_none())
        .map(|action| action.identifier.clone())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

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
    // Missing/absent before setup is fine - `load_or_default` yields an empty
    // config, so `plugin ls` still lists the catalog.
    let config = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?;

    // Catalog actions first (sorted by name), then anything registered locally.
    let mut catalog_entries: Vec<&_> = catalog.actions.iter().collect();
    catalog_entries.sort_by(|a, b| a.name.cmp(&b.name));

    let mut rows: Vec<DisplayRow> = catalog_entries
        .iter()
        .map(|entry| DisplayRow {
            name: entry.name.clone(),
            status: if action_fragment_path(&entry.identifier).exists() {
                Status::Installed
            } else {
                Status::Available
            },
            requires: entry.dependencies.join(", "),
            description: entry.description.clone(),
        })
        .collect();

    for identifier in registered_only(&catalog, &config) {
        rows.push(DisplayRow {
            name: identifier,
            status: Status::Registered,
            requires: String::new(),
            description: String::new(),
        });
    }

    ui::header(
        "Plugins",
        "Catalog actions, plus anything you've registered locally.",
    );
    println!();

    if rows.is_empty() {
        ui::muted_line("No actions available.");
        return Ok(());
    }

    let name_width = rows
        .iter()
        .map(|row| row.name.len())
        .max()
        .unwrap_or(0)
        .max(4);
    let status_width = rows
        .iter()
        .map(|row| row.status.label().len())
        .max()
        .unwrap_or(0)
        .max("STATUS".len());
    let requires_width = rows
        .iter()
        .map(|row| row.requires.len())
        .max()
        .unwrap_or(0)
        .max("REQUIRES".len());

    let description_lines: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            if row.description.is_empty() {
                vec!["-".to_string()]
            } else {
                wrap(&row.description, DESCRIPTION_MAX_WIDTH)
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

    for (entry, description_lines) in rows.iter().zip(&description_lines) {
        let name = style(&entry.name).bold().to_string();
        let status = entry.status.cell();
        let requires = if entry.requires.is_empty() {
            ui::muted().apply_to("-").to_string()
        } else {
            entry.requires.clone()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> ActionCatalog {
        serde_json::from_value(serde_json::json!({
            "actions": [
                { "name": "gls", "identifier": "gls-action",
                  "deployment": { "docker": { "image": "ghcr.io/x/gls-action" } } },
                { "name": "rest-action", "identifier": "rest-action",
                  "deployment": { "docker": { "image": "ghcr.io/x/rest-action" } } }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn registered_only_returns_config_actions_absent_from_the_catalog() {
        let catalog = catalog();
        let mut config = ServiceConfiguration::default();
        config.upsert_action("gls-action", "t1"); // installed catalog action
        config.upsert_action("rest-action", "t2"); // base-stack catalog action
        config.upsert_action("my-dev-action", "t3"); // locally registered, not in catalog

        assert_eq!(
            registered_only(&catalog, &config),
            vec!["my-dev-action".to_string()]
        );
    }

    #[test]
    fn registered_only_is_empty_when_everything_is_a_catalog_action() {
        let catalog = catalog();
        let mut config = ServiceConfiguration::default();
        config.upsert_action("gls-action", "t1");

        assert!(registered_only(&catalog, &config).is_empty());
    }

    #[test]
    fn registered_only_sorts_and_dedupes() {
        let catalog = catalog();
        let mut config = ServiceConfiguration::default();
        config.upsert_action("zeta-action", "t1");
        config.upsert_action("alpha-action", "t2");

        assert_eq!(
            registered_only(&catalog, &config),
            vec!["alpha-action".to_string(), "zeta-action".to_string()]
        );
    }
}
