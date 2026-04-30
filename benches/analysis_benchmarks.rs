//! Benchmark suite for statico analysis pipeline.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::path::Path;

use statico::analyzer;
use statico::duplication::build_clone_groups;
use statico::parse::AstParser;
use statico::parse::exports::extract_exports;
use statico::parse::imports::extract_imports;
use statico::parse::metrics::count_loc;
use statico::types::{CodeBlockLocation, DuplicateCodeIssue};

// ---------------------------------------------------------------------------
// Sample source used for micro-benchmarks
// ---------------------------------------------------------------------------

const SAMPLE_TS: &str = r#"
import { useState, useEffect, useCallback } from 'react';
import { fetchUser, updateUser } from './api/users';
import type { User } from './types';
import * as utils from './utils';

export interface UserProfile {
  id: string;
  name: string;
  email: string;
  role: 'admin' | 'user';
}

export function getDisplayName(user: User): string {
  return user.name || 'Anonymous';
}

export const DEFAULT_PAGE_SIZE = 25;
export type { User };

function internalHelper(x: number): number {
  return x * 2;
}

class UserCache {
  private cache = new Map<string, User>();

  get(id: string): User | undefined {
    return this.cache.get(id);
  }

  set(id: string, user: User): void {
    this.cache.set(id, user);
  }
}
"#;

// ---------------------------------------------------------------------------
// a) Full analysis on fixture projects
// ---------------------------------------------------------------------------

fn bench_analyze_dead_code_project(c: &mut Criterion) {
    let root = Path::new("fixtures/dead-code-project");
    c.bench_function("analyze_dead_code_project", |b| b.iter(|| analyzer::analyze(black_box(root)).unwrap()));
}

fn bench_analyze_nextjs_project(c: &mut Criterion) {
    let root = Path::new("fixtures/nextjs-project");
    c.bench_function("analyze_nextjs_project", |b| b.iter(|| analyzer::analyze(black_box(root)).unwrap()));
}

fn bench_analyze_payload_project(c: &mut Criterion) {
    let root = Path::new("fixtures/payload-project");
    c.bench_function("analyze_payload_project", |b| b.iter(|| analyzer::analyze(black_box(root)).unwrap()));
}

// ---------------------------------------------------------------------------
// c) Individual parsing benchmarks
// ---------------------------------------------------------------------------

fn bench_extract_imports(c: &mut Criterion) {
    let mut parser = AstParser::new().expect("parser init");
    let result = parser.parse(SAMPLE_TS, false).expect("parse");
    let root_node = result.tree.root_node();
    c.bench_function("extract_imports", |b| b.iter(|| extract_imports(black_box(root_node), black_box(SAMPLE_TS))));
}

fn bench_extract_exports(c: &mut Criterion) {
    let mut parser = AstParser::new().expect("parser init");
    let result = parser.parse(SAMPLE_TS, false).expect("parse");
    let root_node = result.tree.root_node();
    c.bench_function("extract_exports", |b| b.iter(|| extract_exports(black_box(root_node), black_box(SAMPLE_TS))));
}

fn bench_count_loc(c: &mut Criterion) {
    c.bench_function("count_loc", |b| b.iter(|| count_loc(black_box(SAMPLE_TS))));
}

fn bench_ast_parse(c: &mut Criterion) {
    let mut parser = AstParser::new().expect("parser init");
    c.bench_function("ast_parse_ts", |b| b.iter(|| parser.parse(black_box(SAMPLE_TS), false)));
}

// ---------------------------------------------------------------------------
// d) Duplication detection
// ---------------------------------------------------------------------------

fn bench_detect_duplicate_groups(c: &mut Criterion) {
    let issues = build_sample_duplication_issues(50);
    c.bench_function("build_clone_groups_50_issues", |b| b.iter(|| build_clone_groups(black_box(&issues))));
}

fn build_sample_duplication_issues(count: usize) -> Vec<DuplicateCodeIssue> {
    (0..count)
        .map(|i| DuplicateCodeIssue {
            confidence: 0.95,
            location_a: CodeBlockLocation {
                file: format!("src/module_a{}.ts", i),
                name: "fn".to_string(),
                start_line: 1 + i * 10,
                end_line: 10 + i * 10,
                snippet: "let x = 1;\nlet y = 2;\nreturn x + y;".to_string(),
            },
            location_b: CodeBlockLocation {
                file: format!("src/module_b{}.ts", i),
                name: "fn".to_string(),
                start_line: 5 + i * 10,
                end_line: 14 + i * 10,
                snippet: "let x = 1;\nlet y = 2;\nreturn x + y;".to_string(),
            },
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Group & main
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_analyze_dead_code_project,
    bench_analyze_nextjs_project,
    bench_analyze_payload_project,
    bench_extract_imports,
    bench_extract_exports,
    bench_count_loc,
    bench_ast_parse,
    bench_detect_duplicate_groups,
);

criterion_main!(benches);
