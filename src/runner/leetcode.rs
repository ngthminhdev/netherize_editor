//! Scaffold templates for the "New LeetCode File" command.
//!
//! Each template is a runnable starter file that reads the whole of stdin and
//! delegates to a `solve` stub, then prints the trimmed result to stdout. The
//! input model is deliberately raw stdin/stdout (universal across problem
//! shapes) — the user parses stdin however the specific problem needs. The
//! comments in each body show the Two Sum convention (line 1 = array, line 2 =
//! target) as a worked example.
//!
//! Only languages the Test Runner can actually execute (see
//! [`super::resolve_run_command`]) are offered here, so every scaffold is
//! immediately runnable with F5.

/// A single language scaffold offered by the New LeetCode File picker.
#[derive(Debug, Clone, Copy)]
pub struct LeetCodeTemplate {
    /// Stable identifier used for the MRU cache and the palette action.
    pub key: &'static str,
    /// Display label in the language picker (e.g. `Python`).
    pub label: &'static str,
    /// Secondary hint shown next to the label (e.g. `python3 · .py`).
    pub hint: &'static str,
    /// File extension WITHOUT the leading dot (e.g. `py`).
    pub extension: &'static str,
    /// Scaffold source written to the new file.
    pub body: &'static str,
}

// Each template keeps the same shape: a `solve(stdin) -> answer` function the
// user fills in, and a tiny `main` harness that reads stdin and prints. The two
// are deliberately separated so the boilerplate stays out of the way.

const PYTHON: &str = r#"import sys


def solve(data: str) -> str:
    # data = the whole stdin. Return the answer as a string.
    return ""


def main() -> None:
    print(solve(sys.stdin.read()).rstrip())


if __name__ == "__main__":
    main()
"#;

const JAVASCRIPT: &str = r#"const { readFileSync } = require("fs");

function solve(data) {
  // Test Runner sends one JSON value to stdin.
  // Customize this destructuring for the problem's parameter names/types.
  const { nums, target } = JSON.parse(data);

  // Example: return twoSum(nums, target);
  return { nums, target };
}

function main() {
  const data = readFileSync(0, "utf8");
  const result = solve(data);
  process.stdout.write(JSON.stringify(result) + "\n");
}

main();
"#;

const TYPESCRIPT: &str = r#"import { readFileSync } from "fs";

function solve(data: string): string {
  // data = the whole stdin. Return the answer as a string.
  return "";
}

function main(): void {
  const data = readFileSync(0, "utf8");
  process.stdout.write(solve(data).trimEnd() + "\n");
}

main();
"#;

const GO: &str = r#"package main

import (
	"fmt"
	"io"
	"os"
	"strings"
)

func solve(data string) string {
	// data = the whole stdin. Return the answer as a string.
	return ""
}

func main() {
	data, _ := io.ReadAll(os.Stdin)
	fmt.Println(strings.TrimRight(solve(string(data)), " \n"))
}
"#;

const RUST: &str = r#"use std::io::{self, Read};

fn solve(data: &str) -> String {
    // data = the whole stdin. Return the answer as a string.
    String::new()
}

fn main() {
    let mut data = String::new();
    io::stdin().read_to_string(&mut data).unwrap();
    println!("{}", solve(&data).trim_end());
}
"#;

const RUBY: &str = r#"def solve(data)
  # data = the whole stdin. Return the answer as a string.
  ""
end

def main
  puts solve(STDIN.read).to_s.rstrip
end

main
"#;

/// All language scaffolds, in their default display order. The MRU cache
/// reorders this list; this order is the fallback for languages never used.
pub fn leetcode_templates() -> &'static [LeetCodeTemplate] {
    &[
        LeetCodeTemplate {
            key: "python",
            label: "Python",
            hint: "python3 · .py",
            extension: "py",
            body: PYTHON,
        },
        LeetCodeTemplate {
            key: "javascript",
            label: "JavaScript",
            hint: "node · .js",
            extension: "js",
            body: JAVASCRIPT,
        },
        LeetCodeTemplate {
            key: "typescript",
            label: "TypeScript",
            hint: "npx tsx · .ts",
            extension: "ts",
            body: TYPESCRIPT,
        },
        LeetCodeTemplate {
            key: "go",
            label: "Go",
            hint: "go run · .go",
            extension: "go",
            body: GO,
        },
        LeetCodeTemplate {
            key: "rust",
            label: "Rust",
            hint: "rustc · .rs",
            extension: "rs",
            body: RUST,
        },
        LeetCodeTemplate {
            key: "ruby",
            label: "Ruby",
            hint: "ruby · .rb",
            extension: "rb",
            body: RUBY,
        },
    ]
}

/// Look up a template by its stable key.
pub fn leetcode_template(key: &str) -> Option<&'static LeetCodeTemplate> {
    leetcode_templates().iter().find(|t| t.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn every_template_is_runnable_by_the_test_runner() {
        // Each scaffold's extension must resolve to a run plan (interpreted or
        // compiled), otherwise F5 would do nothing and the scaffold would be a
        // dead end.
        let out = Path::new("/tmp/netherize_lc_test_bin");
        for template in leetcode_templates() {
            let file = format!("solution.{}", template.extension);
            assert!(
                crate::runner::resolve_run_plan(Path::new(&file), out).is_some(),
                "template {} (.{}) is not runnable",
                template.key,
                template.extension
            );
        }
    }

    #[test]
    fn rust_template_resolves_to_a_compile_step() {
        let out = Path::new("/tmp/netherize_lc_test_bin");
        let plan = crate::runner::resolve_run_plan(Path::new("solution.rs"), out)
            .expect("rust must be runnable");
        let compile = plan.compile.expect("rust must compile first");
        assert_eq!(compile.program, "rustc");
        // The compiled binary is what runs per case, not the .rs source.
        assert_eq!(plan.program, out.to_string_lossy());
        assert!(plan.args.is_empty());
    }

    #[test]
    fn keys_are_unique_and_resolvable() {
        let templates = leetcode_templates();
        for template in templates {
            assert!(leetcode_template(template.key).is_some());
        }
        let mut keys: Vec<&str> = templates.iter().map(|t| t.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), templates.len(), "duplicate template key");
    }

    #[test]
    fn lookup_unknown_key_is_none() {
        assert!(leetcode_template("cobol").is_none());
    }

    #[test]
    fn javascript_template_uses_json_protocol() {
        let template = leetcode_template("javascript").expect("javascript template");
        assert!(template.body.contains("JSON.parse(data)"));
        assert!(template.body.contains("JSON.stringify(result)"));
        assert!(template.body.contains("nums"));
        assert!(template.body.contains("target"));
    }
}
