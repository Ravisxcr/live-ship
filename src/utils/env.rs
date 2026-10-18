use std::collections::HashMap;

/// Parses a multi-line environment string into a HashMap.
/// Handles comments (starting with `#`), empty lines, and quoted values.
pub fn parse_env_string(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let mut val = value.trim().to_string();

            // Strip surrounding quotes if present
            if (val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\''))
            {
                if val.len() >= 2 {
                    val = val[1..val.len() - 1].to_string();
                }
            }

            if !key.is_empty() {
                map.insert(key, val);
            }
        }
    }
    map
}

/// Formats a HashMap of environment variables back to a multi-line `.env` style string.
pub fn format_env_string(map: &HashMap<String, String>) -> String {
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by_key(|&(k, _)| k);

    entries
        .into_iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_format() {
        let input = "PORT=8080\n# comment\nDEBUG=\"true\"\nNODE_ENV=development";
        let map = parse_env_string(input);
        assert_eq!(map.get("PORT"), Some(&"8080".to_string()));
        assert_eq!(map.get("DEBUG"), Some(&"true".to_string()));
        assert_eq!(map.get("NODE_ENV"), Some(&"development".to_string()));
        assert_eq!(map.len(), 3);
    }
}

