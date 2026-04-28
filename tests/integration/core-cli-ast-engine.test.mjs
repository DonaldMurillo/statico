/**
 * Acceptance oracle for core-cli-ast-engine goal.
 *
 * Tests the compiled Rust `statico` binary against all 5 gherkin scenarios.
 * Run via: node --test tests/integration/core-cli-ast-engine.test.mjs
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const projectRoot = join(__dirname, "..", "..");

// Path to the compiled statico binary.
const STATICO_BIN =
	process.env.STATICO_BIN || join(projectRoot, "target", "debug", "statico");

// Path to test fixtures.
const fixtures = (name) => join(projectRoot, "fixtures", name);

// Ensure the binary is built before tests run.
execSync("cargo build", { cwd: projectRoot, stdio: "pipe" });

/**
 * Run `statico analyze <path>` and return { success, stdout, stderr }.
 */
function runAnalyze(path) {
	try {
		const stdout = execSync(`"${STATICO_BIN}" analyze "${path}"`, {
			encoding: "utf-8",
			timeout: 30_000,
			stdio: ["pipe", "pipe", "pipe"],
		});
		return { success: true, stdout, stderr: "" };
	} catch (err) {
		return {
			success: false,
			stdout: err.stdout ?? "",
			stderr: err.stderr ?? "",
		};
	}
}

describe("core-cli-ast-engine", () => {
	it("happy path — analyze a TypeScript project produces valid JSON with all required top-level keys", () => {
		const { success, stdout, stderr } = runAnalyze(fixtures("minimal-ts-project"));

		// Exit code 0.
		assert.ok(success, `expected exit 0, stderr: ${stderr}`);

		// Valid JSON.
		let json;
		assert.doesNotThrow(() => {
			json = JSON.parse(stdout);
		}, `stdout is not valid JSON: ${stdout}`);

		// Top-level keys.
		assert.ok(json.structure, "missing 'structure' key");
		assert.ok(json.dependencies, "missing 'dependencies' key");
		assert.ok(json.quality, "missing 'quality' key");

		// Structure: entry points and source files.
		const entryPoints = json.structure.entry_points;
		assert.ok(Array.isArray(entryPoints), "entry_points should be an array");
		assert.ok(entryPoints.length > 0, "expected at least one entry point");

		const sourceFiles = json.structure.source_files;
		assert.ok(Array.isArray(sourceFiles), "source_files should be an array");
		assert.ok(sourceFiles.length >= 2, `expected at least 2 source files, got ${sourceFiles.length}`);

		// Dependency graph: import relationships.
		const imports = json.dependencies.imports;
		assert.ok(Array.isArray(imports), "imports should be an array");
		assert.ok(imports.length > 0, "expected import relationships");

		// At least one import should have targets (src/index.ts imports utils).
		const hasTargets = imports.some(
			(imp) => Array.isArray(imp.targets) && imp.targets.length > 0,
		);
		assert.ok(hasTargets, "expected at least one import with targets");

		// Quality: complexity metrics per file.
		const qualityFiles = json.quality.files;
		assert.ok(Array.isArray(qualityFiles), "quality.files should be an array");
		assert.ok(qualityFiles.length > 0, "expected quality metrics for files");

		for (const file of qualityFiles) {
			assert.ok(file.metrics, `expected metrics for file ${file.path}`);
			assert.ok(
				file.metrics.complexity !== undefined,
				`expected complexity metric in ${file.path}`,
			);
			assert.ok(
				file.metrics.lines_of_code !== undefined,
				`expected lines_of_code metric in ${file.path}`,
			);
			assert.ok(
				file.metrics.functions !== undefined,
				`expected functions metric in ${file.path}`,
			);
		}
	});

	it("error path — non-existent directory exits non-zero with a clear error message", () => {
		const { success, stdout, stderr } = runAnalyze("/no/such/path/statico-test-nonexistent");

		// Non-zero exit code.
		assert.ok(!success, "expected non-zero exit code");

		// stderr contains human-readable error.
		assert.ok(
			stderr.includes("path not found"),
			`expected 'path not found' in stderr, got: ${stderr}`,
		);

		// stdout is empty (no JSON output).
		assert.ok(
			stdout.trim() === "",
			`expected empty stdout, got: ${stdout}`,
		);
	});

	it("error path — directory with no TypeScript files reports empty analysis gracefully", () => {
		const { success, stdout, stderr } = runAnalyze(fixtures("empty-project"));

		// Exit code 0 for empty projects.
		assert.ok(success, `expected exit 0 for empty project, stderr: ${stderr}`);

		// Valid JSON.
		let json;
		assert.doesNotThrow(() => {
			json = JSON.parse(stdout);
		}, `stdout is not valid JSON: ${stdout}`);

		// Empty sections.
		const sourceFiles = json.structure.source_files;
		assert.ok(Array.isArray(sourceFiles), "source_files should be an array");
		assert.equal(sourceFiles.length, 0, "expected no source files");

		const imports = json.dependencies.imports;
		assert.ok(Array.isArray(imports), "imports should be an array");
		assert.equal(imports.length, 0, "expected no imports");

		const qualityFiles = json.quality.files;
		assert.ok(Array.isArray(qualityFiles), "quality.files should be an array");
		assert.equal(qualityFiles.length, 0, "expected no quality entries");
	});

	it("error path — malformed TypeScript file does not crash the analyzer", () => {
		const { success, stdout, stderr } = runAnalyze(fixtures("malformed-project"));

		// No panic.
		assert.ok(!stderr.includes("panic"), `binary panicked! stderr: ${stderr}`);

		// Exit code 0 (partial analysis success).
		assert.ok(success, `expected exit 0 for partial analysis, stderr: ${stderr}`);

		// Valid JSON.
		let json;
		assert.doesNotThrow(() => {
			json = JSON.parse(stdout);
		}, `stdout is not valid JSON: ${stdout}`);

		// Parse error entries for the broken file.
		const qualityFiles = json.quality.files;
		assert.ok(Array.isArray(qualityFiles), "quality.files should be an array");

		const brokenFile = qualityFiles.find((f) => f.path === "broken.ts");
		assert.ok(brokenFile, "expected broken.ts in quality output");

		const parseErrors = brokenFile.parse_errors;
		assert.ok(Array.isArray(parseErrors), "parse_errors should be an array");
		assert.ok(parseErrors.length > 0, "expected parse errors for broken.ts");

		// Other valid files are still analyzed.
		const validFile = qualityFiles.find((f) => f.path === "valid.ts");
		assert.ok(validFile, "expected valid.ts in quality output");

		const validErrors = validFile.parse_errors;
		assert.ok(Array.isArray(validErrors), "valid.ts parse_errors should be an array");
		assert.equal(validErrors.length, 0, "valid.ts should have no parse errors");

		const validMetrics = validFile.metrics;
		assert.ok(validMetrics, "valid.ts should have metrics");
		assert.ok(
			validMetrics.functions >= 1,
			`valid.ts should have at least 1 function, got ${validMetrics.functions}`,
		);
	});

	it("contract preserved — output JSON schema is deterministic across identical runs", () => {
		const result1 = runAnalyze(fixtures("minimal-ts-project"));
		const result2 = runAnalyze(fixtures("minimal-ts-project"));

		// Both exit 0.
		assert.ok(result1.success, `first run failed: ${result1.stderr}`);
		assert.ok(result2.success, `second run failed: ${result2.stderr}`);

		// Byte-identical output.
		assert.equal(
			result1.stdout,
			result2.stdout,
			"output not deterministic across identical runs",
		);
	});
});
