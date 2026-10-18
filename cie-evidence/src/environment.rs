//! Environment and flag comparison (US-077): configured possibilities are
//! kept separate from observed execution, and exploring a flag change
//! never applies it to any real environment — it returns a new, clearly
//! labelled snapshot and never mutates the one it started from.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A declared environment's deployed version and configured flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentSnapshot {
    pub name: String,
    pub deployed_version: String,
    pub flags: BTreeMap<String, String>,
}

/// One flag whose configured value differs between two environments.
/// `None` means the flag is not configured there at all — distinct from
/// an empty-string value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagDifference {
    pub flag: String,
    pub a_value: Option<String>,
    pub b_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentComparison {
    pub a: String,
    pub b: String,
    pub version_differs: bool,
    /// Sorted by flag name — deterministic, inspectable.
    pub flag_differences: Vec<FlagDifference>,
}

/// Compare two environments' deployed version and configured flags
/// (US-077). This only states configured possibilities; it says nothing
/// about which code path actually ran.
pub fn compare_environments(a: &EnvironmentSnapshot, b: &EnvironmentSnapshot) -> EnvironmentComparison {
    let mut flags: Vec<&String> = a.flags.keys().chain(b.flags.keys()).collect();
    flags.sort();
    flags.dedup();
    let flag_differences = flags
        .into_iter()
        .filter_map(|flag| {
            let av = a.flags.get(flag).cloned();
            let bv = b.flags.get(flag).cloned();
            if av == bv {
                None
            } else {
                Some(FlagDifference { flag: flag.clone(), a_value: av, b_value: bv })
            }
        })
        .collect();
    EnvironmentComparison {
        a: a.name.clone(),
        b: b.name.clone(),
        version_differs: a.deployed_version != b.deployed_version,
        flag_differences,
    }
}

/// Explore a hypothetical flag change: returns a new snapshot named as
/// explored, leaving `base` untouched and applying nowhere (US-077).
pub fn explore_flag_change(base: &EnvironmentSnapshot, flag: &str, value: &str) -> EnvironmentSnapshot {
    let mut flags = base.flags.clone();
    flags.insert(flag.to_string(), value.to_string());
    EnvironmentSnapshot {
        name: format!("{} (explored, not applied)", base.name),
        deployed_version: base.deployed_version.clone(),
        flags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str, version: &str, flags: &[(&str, &str)]) -> EnvironmentSnapshot {
        EnvironmentSnapshot {
            name: name.into(),
            deployed_version: version.into(),
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn finds_flag_and_version_differences() {
        let a = env("staging", "1.0.0", &[("new_checkout", "on"), ("beta", "off")]);
        let b = env("prod", "0.9.0", &[("new_checkout", "off")]);
        let cmp = compare_environments(&a, &b);
        assert!(cmp.version_differs);
        assert_eq!(cmp.flag_differences.len(), 2); // new_checkout differs, beta only on a
        let nc = cmp.flag_differences.iter().find(|d| d.flag == "new_checkout").unwrap();
        assert_eq!(nc.a_value, Some("on".to_string()));
        assert_eq!(nc.b_value, Some("off".to_string()));
        let beta = cmp.flag_differences.iter().find(|d| d.flag == "beta").unwrap();
        assert_eq!(beta.b_value, None);
    }

    #[test]
    fn identical_environments_have_no_differences() {
        let a = env("x", "1.0.0", &[("f", "on")]);
        let b = env("y", "1.0.0", &[("f", "on")]);
        let cmp = compare_environments(&a, &b);
        assert!(!cmp.version_differs);
        assert!(cmp.flag_differences.is_empty());
    }

    #[test]
    fn exploring_a_flag_change_never_mutates_the_base() {
        let base = env("prod", "1.0.0", &[("f", "off")]);
        let explored = explore_flag_change(&base, "f", "on");
        assert_eq!(base.flags["f"], "off");
        assert_eq!(explored.flags["f"], "on");
        assert!(explored.name.contains("not applied"));
    }
}
