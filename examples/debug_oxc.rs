use statico::resolution::Resolver;

fn main() {
    let root = std::path::Path::new("benchmarks/repos/shadcn");

    // Test some imports that should resolve
    let test_cases = [
        ("packages/ui/src/button.tsx", "@test/utils"),
        ("apps/docs/app/page.tsx", "@/components/ui/button"),
        ("apps/docs/app/page.tsx", "./layout"),
        ("packages/ui/src/button.tsx", "react"),
    ];

    // First: our resolver
    println!("=== Our resolver ===");
    let mut resolver = Resolver::new(root);
    resolver.load_workspace_packages();
    for (file, spec) in &test_cases {
        let abs = root.join(file);
        let file_dir = abs.parent().unwrap();
        let result = resolver.resolve(file_dir, spec);
        println!("  {} from {}", spec, result.map(|p| p.to_string_lossy().to_string()).unwrap_or("NONE".into()));
    }

    // Also test with oxc (requires deep-resolution feature)
    #[cfg(feature = "deep-resolution")]
    {
        use oxc_resolver::{AliasValue, ResolveOptions};

        println!("\n=== oxc_resolver (with workspace aliases) ===");
        let mut workspace_aliases: Vec<(String, Vec<AliasValue>)> = Vec::new();
        for entry in walkdir::WalkDir::new(root).max_depth(5).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.file_name().map_or(true, |n| n != "package.json") {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
            if rel.contains("node_modules") {
                continue;
            }
            let content = std::fs::read_to_string(path).unwrap_or_default();
            let pkg: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
            let name = match pkg.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => continue,
            };
            if !name.starts_with('@') && !name.contains('/') {
                continue;
            }
            let pkg_dir = path.parent().unwrap();
            workspace_aliases
                .push((format!("{}$", name), vec![AliasValue::Path(pkg_dir.to_string_lossy().to_string())]));
            workspace_aliases.push((name.to_string(), vec![AliasValue::Path(pkg_dir.to_string_lossy().to_string())]));
        }
        println!("  ({} workspace aliases)", workspace_aliases.len());
        let options2 = ResolveOptions {
            extensions: vec![
                ".ts".into(),
                ".tsx".into(),
                ".js".into(),
                ".jsx".into(),
                ".mjs".into(),
                ".cjs".into(),
                ".d.ts".into(),
            ],
            main_fields: vec!["module".into(), "main".into(), "types".into(), "typings".into()],
            main_files: vec!["index".into()],
            condition_names: vec![
                "import".into(),
                "module".into(),
                "require".into(),
                "default".into(),
                "types".into(),
                "node".into(),
            ],
            tsconfig: None,
            alias: workspace_aliases,
            prefer_relative: true,
            ..Default::default()
        };
        let oxc2 = oxc_resolver::Resolver::new(options2);
        for (file, spec) in &test_cases {
            let abs = root.join(file);
            let file_dir = abs.parent().unwrap();
            let result = oxc2.resolve(file_dir, spec);
            println!(
                "  {} from {}",
                spec,
                result.map(|r| r.full_path().to_string_lossy().to_string()).unwrap_or_else(|e| format!("ERR: {}", e))
            );
        }
    }

    #[cfg(not(feature = "deep-resolution"))]
    println!("\n(Note: oxc_resolver comparison requires --features deep-resolution)");
}
