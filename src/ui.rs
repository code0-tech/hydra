use console::{Style, style};
use dialoguer::theme::ColorfulTheme;

pub fn accent() -> Style {
    Style::new().color256(45)
}

pub fn secondary() -> Style {
    Style::new().color256(141)
}

pub fn success() -> Style {
    Style::new().color256(78)
}

pub fn warning() -> Style {
    Style::new().color256(214)
}

pub fn error() -> Style {
    Style::new().color256(203)
}

pub fn muted() -> Style {
    Style::new().color256(244)
}

pub fn wizard_theme() -> ColorfulTheme {
    ColorfulTheme {
        defaults_style: muted().for_stderr(),
        prompt_style: Style::new().for_stderr().bold(),
        prompt_prefix: style("❯".to_string()).for_stderr().color256(45).bold(),
        prompt_suffix: style("".to_string()).for_stderr(),
        success_prefix: style("✓".to_string()).for_stderr().color256(78).bold(),
        success_suffix: style("".to_string()).for_stderr(),
        error_prefix: style("✗".to_string()).for_stderr().color256(203).bold(),
        error_style: error().for_stderr(),
        hint_style: muted().for_stderr(),
        values_style: accent().for_stderr(),
        active_item_style: accent().for_stderr().bold(),
        inactive_item_style: Style::new().for_stderr(),
        active_item_prefix: style("❯ ".to_string()).for_stderr().color256(45).bold(),
        inactive_item_prefix: style("  ".to_string()).for_stderr(),
        checked_item_prefix: style("◉ ".to_string()).for_stderr().color256(78),
        unchecked_item_prefix: style("○ ".to_string()).for_stderr().color256(244),
        picked_item_prefix: style("❯ ".to_string()).for_stderr().color256(45).bold(),
        unpicked_item_prefix: style("  ".to_string()).for_stderr(),
    }
}

/// Bold, brand-accent page title with a muted subtitle beneath it.
pub fn header(title: &str, subtitle: &str) {
    println!();
    println!("{}", accent().bold().apply_to(title));
    println!("{}", muted().apply_to(subtitle));
}

/// A step header for a multi-section wizard: `[index/total]` followed by the
/// section title. Kept as a plain counter (rather than a dot tracker) since it
/// needs to stay readable as the number of sections grows.
pub fn section(index: usize, total: usize, title: &str) {
    println!();
    println!(
        "{} {}",
        accent().bold().apply_to(format!("[{index}/{total}]")),
        style(title).bold()
    );
}

pub fn success_line(text: &str) {
    println!("{} {}", success().bold().apply_to("✓"), text);
}

pub fn warn_line(text: &str) {
    println!("{} {}", warning().bold().apply_to("!"), text);
}

pub fn muted_line(text: &str) {
    println!("{}", muted().apply_to(text));
}
