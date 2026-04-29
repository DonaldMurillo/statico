//! Duplicate export detection — same name exported from multiple files.

use std::collections::BTreeMap;

use crate::types::DuplicateExportIssue;

/// Export names that are framework conventions — expected to appear in many files.
const FRAMEWORK_CONVENTIONS: &[&str] = &[
    // Next.js route handlers
    "GET",
    "POST",
    "PUT",
    "DELETE",
    "PATCH",
    "HEAD",
    "OPTIONS",
    // Next.js page/layout exports
    "metadata",
    "generateMetadata",
    "dynamic",
    "revalidate",
    "dynamicParams",
    // Payload/Next.js config exports
    "default",
    // Migration conventions
    "up",
    "down",
    // Common barrel re-export names
    "config",
];

/// Check if an export name is a framework convention.
fn is_framework_convention(name: &str) -> bool {
    FRAMEWORK_CONVENTIONS.contains(&name)
}

/// Check if ALL files for this export are in a convention-heavy directory
/// (migrations, route handlers, etc.).
fn all_in_convention_dirs(files: &[String]) -> bool {
    files.iter().all(|f| {
        f.contains("/migrations/")
            || f.contains("/route.ts")
            || f.contains("/route.tsx")
            || f.contains("/route.js")
    })
}

/// Find non-default export names that appear in more than one file.
/// Filters out framework conventions that are expected to repeat.
pub fn detect(file_exports: &BTreeMap<String, Vec<String>>) -> Vec<DuplicateExportIssue> {
    let mut name_to_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, exports) in file_exports {
        for name in exports {
            name_to_files
                .entry(name.clone())
                .or_default()
                .push(path.clone());
        }
    }

    let mut duplicates: Vec<DuplicateExportIssue> = name_to_files
        .into_iter()
        .filter(|(name, files)| {
            files.len() > 1
                && name != "default"
                && !is_framework_convention(name)
                // If all files are route handlers, HTTP method duplicates are expected.
                && !(matches!(name.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "PATCH")
                    && all_in_convention_dirs(files))
        })
        .map(|(name, mut files)| {
            files.sort();
            DuplicateExportIssue {
                name,
                locations: files,
            }
        })
        .collect();

    duplicates.sort_by(|a, b| a.name.cmp(&b.name));
    duplicates
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_duplicate_name() {
        let exports = BTreeMap::from([
            ("src/a.ts".into(), vec!["foo".into(), "bar".into()]),
            ("src/b.ts".into(), vec!["foo".into()]),
            ("src/c.ts".into(), vec!["baz".into()]),
        ]);
        let dupes = detect(&exports);
        assert_eq!(dupes.len(), 1);
        assert_eq!(dupes[0].name, "foo");
        assert!(dupes[0].locations.contains(&"src/a.ts".into()));
        assert!(dupes[0].locations.contains(&"src/b.ts".into()));
    }

    #[test]
    fn skips_default_exports() {
        let exports = BTreeMap::from([
            ("src/a.ts".into(), vec!["default".into()]),
            ("src/b.ts".into(), vec!["default".into()]),
        ]);
        assert!(detect(&exports).is_empty());
    }

    #[test]
    fn no_duplicates() {
        let exports = BTreeMap::from([
            ("src/a.ts".into(), vec!["alpha".into()]),
            ("src/b.ts".into(), vec!["beta".into()]),
        ]);
        assert!(detect(&exports).is_empty());
    }
}
