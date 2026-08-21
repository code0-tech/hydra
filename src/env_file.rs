use std::{collections::HashMap, fs, path::Path};

/// Parses a `KEY=value` env file into a map, skipping blank lines and
/// comments. Used to seed prompts with the current value and to read values
/// back out (e.g. before overwriting `IMAGE_TAG`).
pub fn read_all(path: impl AsRef<Path>) -> anyhow::Result<HashMap<String, String>> {
    let contents = fs::read_to_string(path)?;

    Ok(contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect())
}

pub fn read_value(path: impl AsRef<Path>, key: &str) -> anyhow::Result<Option<String>> {
    Ok(read_all(path)?.remove(key))
}

/// Replaces (or appends) `KEY=value` lines, leaving every other line -
/// comments, ordering, unrelated settings - untouched.
pub fn apply_values(contents: &str, updates: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = contents.lines().map(str::to_string).collect();

    for (key, value) in updates {
        let prefix = format!("{key}=");
        match lines.iter_mut().find(|line| line.starts_with(&prefix)) {
            Some(line) => *line = format!("{key}={value}"),
            None => lines.push(format!("{key}={value}")),
        }
    }

    let mut rendered = lines.join("\n");
    rendered.push('\n');
    rendered
}

/// Replaces (or appends) `KEY=value` lines in an env file in place.
pub fn set_values(path: impl AsRef<Path>, updates: &[(&str, &str)]) -> anyhow::Result<()> {
    let contents = fs::read_to_string(&path)?;
    fs::write(path, apply_values(&contents, updates))?;
    Ok(())
}

/// Deletes `KEY=value` lines for the given keys, leaving every other line
/// untouched. Keys that aren't present are a no-op.
pub fn remove_keys(path: impl AsRef<Path>, keys: &[&str]) -> anyhow::Result<()> {
    let contents = fs::read_to_string(&path)?;
    let rendered = contents
        .lines()
        .filter(|line| !keys.iter().any(|key| line.starts_with(&format!("{key}="))))
        .collect::<Vec<_>>()
        .join("\n");
    let mut rendered = rendered;
    rendered.push('\n');
    fs::write(path, rendered)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_an_existing_key_in_place_and_leaves_others_untouched() {
        let path = std::env::temp_dir().join(format!("hydra-env-file-test-{}-a.env", std::process::id()));
        fs::write(&path, "HOSTNAME=localhost\nIMAGE_TAG=old\nSSL_ENABLED=false\n").unwrap();

        set_values(&path, &[("IMAGE_TAG", "new")]).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "HOSTNAME=localhost\nIMAGE_TAG=new\nSSL_ENABLED=false\n");

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn appends_the_key_when_it_is_missing() {
        let path = std::env::temp_dir().join(format!("hydra-env-file-test-{}-b.env", std::process::id()));
        fs::write(&path, "HOSTNAME=localhost\n").unwrap();

        set_values(&path, &[("IMAGE_TAG", "new")]).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "HOSTNAME=localhost\nIMAGE_TAG=new\n");

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reads_an_existing_value() {
        let path = std::env::temp_dir().join(format!("hydra-env-file-test-{}-c.env", std::process::id()));
        fs::write(&path, "HOSTNAME=localhost\nIMAGE_TAG=abc123\n").unwrap();

        let value = read_value(&path, "IMAGE_TAG").unwrap();

        assert_eq!(value, Some("abc123".to_string()));
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reads_none_for_a_missing_key() {
        let path = std::env::temp_dir().join(format!("hydra-env-file-test-{}-d.env", std::process::id()));
        fs::write(&path, "HOSTNAME=localhost\n").unwrap();

        let value = read_value(&path, "IMAGE_TAG").unwrap();

        assert_eq!(value, None);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn removes_matching_keys_and_leaves_others_untouched() {
        let path = std::env::temp_dir().join(format!("hydra-env-file-test-{}-f.env", std::process::id()));
        fs::write(&path, "HOSTNAME=localhost\nAQUILA_ACTION_GLS_IDENTIFIER=gls-action\nAQUILA_ACTION_GLS_TOKEN=secret\n").unwrap();

        remove_keys(&path, &["AQUILA_ACTION_GLS_IDENTIFIER", "AQUILA_ACTION_GLS_TOKEN"]).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "HOSTNAME=localhost\n");

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn read_all_skips_comments_and_blank_lines() {
        let path = std::env::temp_dir().join(format!("hydra-env-file-test-{}-e.env", std::process::id()));
        fs::write(&path, "# a comment\n\nHOSTNAME=localhost\n  \nIMAGE_TAG=abc\n").unwrap();

        let map = read_all(&path).unwrap();

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("HOSTNAME"), Some(&"localhost".to_string()));
        assert_eq!(map.get("IMAGE_TAG"), Some(&"abc".to_string()));

        fs::remove_file(&path).unwrap();
    }
}
