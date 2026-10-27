//! What can be configured: the *configurable* of Salus or of a plugin.
//!
//! A configurable is a TOML file that is never changed at runtime (Salus embeds its own, a plugin names
//! its file in the manifest under `[config]`). It lists the settings with their type, a display name and
//! an explanation:
//!
//! ```toml
//! [[setting]]
//! key = "zoom"                  # name in the config file, [A-Za-z0-9_-]+
//! name = "Zoom step"            # display name
//! description = "How far one click zooms."
//! type = "float"                # int, float, bool, string, choice, file-path or directory-path
//! default = 1.5                 # optional; a setting without one is simply unset
//! min = 1.0                     # int and float only, optional
//! max = 4.0
//! # choices = ["a", "b"]        # choice only, required
//! ```
//!
//! The values live in one config file per plugin (see `config_loader`). This module knows nothing about files:
//! it parses a configurable and checks values against it.

use serde_json::{json, Map, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingType {
    Int,
    Float,
    Bool,
    String,
    Choice,
    FilePath,
    DirectoryPath,
}

impl SettingType {
    pub const ALL: [(&'static str, SettingType); 7] = [
        ("int", SettingType::Int),
        ("float", SettingType::Float),
        ("bool", SettingType::Bool),
        ("string", SettingType::String),
        ("choice", SettingType::Choice),
        ("file-path", SettingType::FilePath),
        ("directory-path", SettingType::DirectoryPath),
    ];

    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|(_, t)| *t == self).map(|(name, _)| *name).unwrap()
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub key: String,
    pub name: String,
    pub description: String,
    pub kind: SettingType,
    pub default: Option<Value>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Configurable {
    pub settings: Vec<Setting>,
}

impl Setting {
    /// Checks `value` against the type and returns it in its canonical form (a float setting always holds a float).
    pub fn validate(&self, value: &Value) -> Result<Value, String> {
        let fail = |expected: &str| Err(format!("'{}' expects {expected}", self.key));
        match self.kind {
            SettingType::Int => {
                let Some(number) = value.as_i64() else { return fail("an integer") };
                self.check_range(number as f64)?;
                Ok(json!(number))
            }
            SettingType::Float => {
                let Some(number) = value.as_f64().filter(|n| n.is_finite()) else { return fail("a number") };
                self.check_range(number)?;
                Ok(json!(number))
            }
            SettingType::Bool => match value {
                Value::Bool(_) => Ok(value.clone()),
                _ => fail("true or false"),
            },
            SettingType::String | SettingType::FilePath | SettingType::DirectoryPath => match value {
                Value::String(_) => Ok(value.clone()),
                _ => fail("a string"),
            },
            SettingType::Choice => match value.as_str() {
                Some(choice) if self.choices.iter().any(|c| c == choice) => Ok(value.clone()),
                _ => Err(format!("'{}' expects one of: {}", self.key, self.choices.join(", "))),
            },
        }
    }

    fn check_range(&self, number: f64) -> Result<(), String> {
        if self.min.is_some_and(|min| number < min) || self.max.is_some_and(|max| number > max) {
            let bound = |b: Option<f64>| b.map_or("unbounded".to_string(), |b| b.to_string());
            return Err(format!("'{}' must be between {} and {}", self.key, bound(self.min), bound(self.max)));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        let mut setting = json!({
            "key": self.key,
            "name": self.name,
            "description": self.description,
            "type": self.kind.name(),
        });
        let object = setting.as_object_mut().unwrap();
        if let Some(default) = &self.default {
            object.insert("default".into(), default.clone());
        }
        if let Some(min) = self.min {
            object.insert("min".into(), json!(min));
        }
        if let Some(max) = self.max {
            object.insert("max".into(), json!(max));
        }
        if self.kind == SettingType::Choice {
            object.insert("choices".into(), json!(self.choices));
        }
        setting
    }
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl Configurable {
    pub fn setting(&self, key: &str) -> Option<&Setting> {
        self.settings.iter().find(|setting| setting.key == key)
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.settings.iter().map(Setting::to_json).collect())
    }

    /// Parses the text of a configurable file. Errors name the offending setting.
    pub fn parse(text: &str) -> Result<Configurable, String> {
        let table: toml::Table = text.parse().map_err(|e: toml::de::Error| format!("not valid TOML: {e}"))?;
        if let Some(unknown) = table.keys().find(|key| key.as_str() != "setting") {
            return Err(format!("unknown top-level key '{unknown}'; settings are declared as [[setting]] tables"));
        }
        let entries = match table.get("setting") {
            None => return Ok(Configurable::default()),
            Some(toml::Value::Array(entries)) => entries,
            Some(_) => return Err("'setting' must be an array of [[setting]] tables".into()),
        };

        let mut settings: Vec<Setting> = Vec::new();
        let mut seen = HashSet::new();
        for (index, entry) in entries.iter().enumerate() {
            let table = entry.as_table().ok_or_else(|| format!("setting #{}: must be a table", index + 1))?;
            let setting = parse_setting(table).map_err(|e| {
                let label = table.get("key").and_then(|k| k.as_str()).map_or(format!("#{}", index + 1), |k| format!("'{k}'"));
                format!("setting {label}: {e}")
            })?;
            if !seen.insert(setting.key.clone()) {
                return Err(format!("setting '{}' is declared twice", setting.key));
            }
            settings.push(setting);
        }
        Ok(Configurable { settings })
    }

    /// The values in effect: each setting's stored value, else its default; settings without either are left out.
    /// A stored value that no longer fits (the configurable changed) is ignored.
    pub fn effective(&self, stored: &Map<String, Value>) -> Map<String, Value> {
        let mut values = Map::new();
        for setting in &self.settings {
            let value = stored
                .get(&setting.key)
                .and_then(|value| setting.validate(value).ok())
                .or_else(|| setting.default.clone());
            if let Some(value) = value {
                values.insert(setting.key.clone(), value);
            }
        }
        values
    }

    /// Applies `updates` to the stored values: a value sets the key, `null` removes it (back to the default).
    /// Nothing is changed if any update is invalid.
    pub fn apply(&self, stored: &Map<String, Value>, updates: &Map<String, Value>) -> Result<Map<String, Value>, String> {
        let mut result = stored.clone();
        for (key, value) in updates {
            let setting = self.setting(key).ok_or_else(|| format!("unknown setting '{key}'"))?;
            if value.is_null() {
                result.remove(key);
            } else {
                result.insert(key.clone(), setting.validate(value)?);
            }
        }
        Ok(result)
    }
}

fn parse_setting(table: &toml::Table) -> Result<Setting, String> {
    const KNOWN: [&str; 8] = ["key", "name", "description", "type", "default", "min", "max", "choices"];
    if let Some(unknown) = table.keys().find(|key| !KNOWN.contains(&key.as_str())) {
        return Err(format!("unknown key '{unknown}'"));
    }
    let string = |key: &str| -> Result<Option<String>, String> {
        match table.get(key) {
            None => Ok(None),
            Some(toml::Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("'{key}' must be a string")),
        }
    };
    let key = string("key")?.ok_or("'key' is required")?;
    if !valid_key(&key) {
        return Err(format!("key '{key}' must match [A-Za-z0-9_-]+"));
    }
    let name = string("name")?.filter(|n| !n.is_empty()).ok_or("'name' (the display name) is required")?;
    let description = string("description")?.unwrap_or_default();
    let type_name = string("type")?.ok_or("'type' is required")?;
    let kind = SettingType::parse(&type_name).ok_or_else(|| {
        let known: Vec<_> = SettingType::ALL.iter().map(|(n, _)| *n).collect();
        format!("unknown type '{type_name}' (use one of: {})", known.join(", "))
    })?;

    let number = |key: &str| -> Result<Option<f64>, String> {
        match table.get(key) {
            None => Ok(None),
            Some(toml::Value::Integer(i)) => Ok(Some(*i as f64)),
            Some(toml::Value::Float(f)) => Ok(Some(*f)),
            Some(_) => Err(format!("'{key}' must be a number")),
        }
    };
    let (min, max) = (number("min")?, number("max")?);
    if (min.is_some() || max.is_some()) && !matches!(kind, SettingType::Int | SettingType::Float) {
        return Err("'min' and 'max' only apply to int and float settings".into());
    }
    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            return Err("'min' is larger than 'max'".into());
        }
    }

    let mut choices = Vec::new();
    match (table.get("choices"), kind) {
        (None, SettingType::Choice) => return Err("a choice setting needs 'choices'".into()),
        (None, _) => {}
        (Some(_), kind) if kind != SettingType::Choice => return Err("'choices' only applies to choice settings".into()),
        (Some(toml::Value::Array(list)), _) => {
            for item in list {
                let choice = item.as_str().ok_or("'choices' must be an array of strings")?;
                if choices.iter().any(|c| c == choice) {
                    return Err(format!("choice '{choice}' is listed twice"));
                }
                choices.push(choice.to_string());
            }
            if choices.is_empty() {
                return Err("'choices' must not be empty".into());
            }
        }
        (Some(_), _) => return Err("'choices' must be an array of strings".into()),
    }

    let mut setting = Setting { key, name, description, kind, default: None, min, max, choices };
    if let Some(default) = table.get("default") {
        let default = serde_json::to_value(default).map_err(|e| format!("'default': {e}"))?;
        setting.default = Some(setting.validate(&default).map_err(|e| format!("default: {e}"))?);
    }
    Ok(setting)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[setting]]
key = "zoom"
name = "Zoom step"
description = "How far one click zooms."
type = "float"
default = 1.5
min = 1
max = 4

[[setting]]
key = "count"
name = "Count"
type = "int"

[[setting]]
key = "mode"
name = "Mode"
type = "choice"
choices = ["fast", "exact"]
default = "fast"

[[setting]]
key = "dir"
name = "Directory"
type = "directory-path"
"#;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn parses_all_the_parts_of_a_setting() {
        let c = Configurable::parse(SAMPLE).unwrap();
        assert_eq!(c.settings.len(), 4);
        let zoom = c.setting("zoom").unwrap();
        assert_eq!((zoom.kind, zoom.min, zoom.max), (SettingType::Float, Some(1.0), Some(4.0)));
        assert_eq!(zoom.default, Some(json!(1.5)));
        assert_eq!(zoom.description, "How far one click zooms.");
        assert_eq!(c.setting("count").unwrap().default, None);
        assert_eq!(c.setting("mode").unwrap().choices, ["fast", "exact"]);
        assert_eq!(c.to_json()[0]["type"], "float");
        assert_eq!(Configurable::parse("").unwrap(), Configurable::default());
    }

    #[test]
    fn rejects_bad_configurables() {
        let bad = |extra: &str| Configurable::parse(&format!("[[setting]]\nkey = \"k\"\nname = \"N\"\n{extra}")).unwrap_err();
        assert!(bad("type = \"nope\"").contains("unknown type"));
        assert!(bad("").contains("'type' is required"));
        assert!(bad("type = \"int\"\ndefault = 1.5").contains("integer"));
        assert!(bad("type = \"int\"\nmin = 5\ndefault = 1").contains("between"));
        assert!(bad("type = \"string\"\nmin = 1").contains("only apply"));
        assert!(bad("type = \"choice\"").contains("needs 'choices'"));
        assert!(bad("type = \"choice\"\nchoices = [\"a\"]\ndefault = \"b\"").contains("one of"));
        assert!(bad("type = \"bool\"\nchoices = [\"a\"]").contains("only applies"));
        assert!(bad("type = \"bool\"\nunknown = 1").contains("unknown key"));
        assert!(Configurable::parse("[[setting]]\nkey = \"a b\"\nname = \"N\"\ntype = \"bool\"").unwrap_err().contains("must match"));
        assert!(Configurable::parse("[[setting]]\nkey = \"a\"\ntype = \"bool\"").unwrap_err().contains("display name"));
        let twice = "[[setting]]\nkey = \"a\"\nname = \"A\"\ntype = \"bool\"\n";
        assert!(Configurable::parse(&format!("{twice}{twice}")).unwrap_err().contains("twice"));
        assert!(Configurable::parse("x = 1").is_err());
    }

    #[test]
    fn validates_values() {
        let c = Configurable::parse(SAMPLE).unwrap();
        let zoom = c.setting("zoom").unwrap();
        assert_eq!(zoom.validate(&json!(2)).unwrap(), json!(2.0)); // an integer is fine for a float
        assert!(zoom.validate(&json!(5)).is_err());
        assert!(zoom.validate(&json!("2")).is_err());
        let count = c.setting("count").unwrap();
        assert!(count.validate(&json!(3)).is_ok());
        assert!(count.validate(&json!(3.5)).is_err());
        assert!(c.setting("mode").unwrap().validate(&json!("slow")).is_err());
        assert!(c.setting("dir").unwrap().validate(&json!("/tmp")).is_ok());
        assert!(c.setting("dir").unwrap().validate(&json!(true)).is_err());
    }

    #[test]
    fn effective_values_fall_back_to_defaults() {
        let c = Configurable::parse(SAMPLE).unwrap();
        let values = c.effective(&map(json!({"zoom": 2.0, "count": 7, "gone": 1, "mode": "invalid"})));
        assert_eq!(Value::Object(values), json!({"zoom": 2.0, "count": 7, "mode": "fast"}));
        assert_eq!(Value::Object(c.effective(&Map::new())), json!({"zoom": 1.5, "mode": "fast"}));
    }

    #[test]
    fn apply_sets_resets_and_is_all_or_nothing() {
        let c = Configurable::parse(SAMPLE).unwrap();
        let stored = map(json!({"count": 1, "zoom": 2.0}));
        let updated = c.apply(&stored, &map(json!({"count": 9, "zoom": null, "mode": "exact"}))).unwrap();
        assert_eq!(Value::Object(updated), json!({"count": 9, "mode": "exact"}));
        assert!(c.apply(&stored, &map(json!({"count": 9, "mode": "slow"}))).is_err());
        assert!(c.apply(&stored, &map(json!({"nope": 1}))).unwrap_err().contains("unknown setting"));
    }
}
