//! Shared troop-level formatting for Lookup tabs (JS `services/troopLevels.js`).

use serde_json::Value;

fn number(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64).or_else(|| {
        value
            .and_then(Value::as_str)
            .and_then(|text| text.parse::<i64>().ok())
    })
}

fn field<'a>(raw: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| raw.get(*key))
}

/// Render a compact "Infantry · T12 · FC10 / …" summary for one member.
pub fn troop_summary(raw: &Value) -> String {
    let mut bits = Vec::new();
    let troops_value = field(raw, &["troop", "troops", "troop_levels", "troopLevels"]);
    if let Some(value) = troops_value {
        if let Some(rows) = value.as_array() {
            for troop in rows {
                let kind = field(troop, &["kind", "name", "type"])
                    .and_then(Value::as_str)
                    .unwrap_or("troop");
                let tier = number(field(troop, &["tier", "level"]));
                if let Some(tier) = tier {
                    let fc = number(field(troop, &["fc", "star", "fortress"]));
                    bits.push(match fc {
                        Some(fc) => format!("{kind} · T{tier} · FC{fc}"),
                        None => format!("{kind} · T{tier}"),
                    });
                }
            }
        } else if let Some(map) = value.as_object() {
            for (name, level) in map {
                if let Some(level) = number(Some(level)) {
                    let label = match name.as_str() {
                        "inf" | "infantry" => "Infantry",
                        "lan" | "lancer" => "Lancer",
                        "mm" | "marksman" => "Marksman",
                        other => other,
                    };
                    bits.push(format!("{label} · T{level}"));
                }
            }
        }
    }
    if bits.is_empty() {
        for (name, keys) in [
            ("Infantry", &["infantry", "inf", "infantry_level"][..]),
            ("Lancer", &["lancer", "lan", "lancer_level"][..]),
            ("Marksman", &["marksman", "mm", "marksman_level"][..]),
        ] {
            if let Some(level) = number(field(raw, keys)) {
                bits.push(format!("{name} · T{level}"));
            }
        }
    }
    if bits.is_empty() {
        "_".to_string()
    } else {
        bits.join(" / ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formats_troop_array_and_fallback() {
        let raw = json!({ "troop_levels": { "infantry": 12, "lancer": 11 } });
        assert_eq!(troop_summary(&raw), "Infantry · T12 / Lancer · T11");
        let raw = json!({ "troop": [{ "kind": "inf", "tier": 13, "fc": 10 }] });
        assert_eq!(troop_summary(&raw), "inf · T13 · FC10");
        assert_eq!(troop_summary(&json!({})), "_");
    }
}
