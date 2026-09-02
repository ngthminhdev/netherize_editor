use super::leetcode_api::LeetCodeProblem;

pub fn language_key_for_extension(extension: &str) -> Option<&'static str> {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "py" => Some("python"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "go" => Some("go"),
        "rs" => Some("rust"),
        "rb" => Some("ruby"),
        _ => None,
    }
}

pub fn adapt_snippet_mechanical(
    problem: &LeetCodeProblem,
    language_key: &str,
) -> Result<String, String> {
    let snippet_slug = match language_key {
        "python" => "python3",
        "javascript" => "javascript",
        "typescript" => "typescript",
        "go" => "golang",
        "rust" => "rust",
        "ruby" => "ruby",
        _ => return Err(format!("unsupported LeetCode language: {language_key}")),
    };
    let snippet = problem
        .code_snippets
        .iter()
        .find(|snippet| snippet.lang_slug == snippet_slug)
        .ok_or_else(|| format!("LeetCode returned no {language_key} snippet"))?;
    let names: Vec<&str> = problem
        .parameters
        .iter()
        .map(|param| param.name.as_str())
        .collect();
    let member_args = names
        .iter()
        .map(|name| format!("params.{name}"))
        .collect::<Vec<_>>()
        .join(", ");
    let index_args = names
        .iter()
        .map(|name| format!("params[\"{name}\"]"))
        .collect::<Vec<_>>()
        .join(", ");
    let go_fields = problem
        .parameters
        .iter()
        .map(|param| {
            format!(
                "    {} {} `json:\"{}\"`",
                exported_name(&param.name),
                go_type(&param.type_name),
                param.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let go_args = problem
        .parameters
        .iter()
        .map(|param| format!("params.{}", exported_name(&param.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let rust_method =
        rust_method_name(&snippet.code).unwrap_or_else(|| problem.function_name.clone());
    let rust_bindings = problem
        .parameters
        .iter()
        .map(|param| rust_binding(&param.name, &param.type_name))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n    ");
    let code = match language_key {
        "javascript" | "typescript" => format!(
            "const {{ readFileSync }} = require(\"fs\");\n\n{}\n\nfunction solve(data) {{\n  const params = JSON.parse(data);\n  return {}({});\n}}\n\nconst result = solve(readFileSync(0, \"utf8\"));\nprocess.stdout.write(JSON.stringify(result) + \"\\n\");\n",
            snippet.code, problem.function_name, member_args
        ),
        "python" => format!(
            "import json\nimport sys\nfrom typing import *\n\n{}\n\ndef solve(data: str) -> str:\n    params = json.loads(data)\n    result = Solution().{}({})\n    return json.dumps(result, separators=(\",\", \":\"))\n\nif __name__ == \"__main__\":\n    print(solve(sys.stdin.read()).rstrip())\n",
            snippet.code, problem.function_name, index_args
        ),
        "go" => format!(
            "package main\n\nimport (\n    \"encoding/json\"\n    \"os\"\n)\n\n{}\n\ntype inputParams struct {{\n{}\n}}\n\nfunc solve() error {{\n    var params inputParams\n    if err := json.NewDecoder(os.Stdin).Decode(&params); err != nil {{ return err }}\n    result := {}({})\n    return json.NewEncoder(os.Stdout).Encode(result)\n}}\n\nfunc main() {{\n    if err := solve(); err != nil {{ panic(err) }}\n}}\n",
            snippet.code, go_fields, problem.function_name, go_args
        ),
        "rust" => format!(
            "use std::io::{{self, Read}};\n\npub struct Solution;\n\n{}\n\nfn json_field(data: &str, name: &str) -> String {{\n    let needle = format!(\"\\\"{{}}\\\"\", name);\n    let Some(start) = data.find(&needle).map(|index| index + needle.len()) else {{ return String::new(); }};\n    let Some(colon) = data[start..].find(':').map(|index| start + index + 1) else {{ return String::new(); }};\n    let tail = data[colon..].trim_start();\n    let mut depth = 0i32;\n    let mut quoted = false;\n    let mut escaped = false;\n    for (index, ch) in tail.char_indices() {{\n        if quoted {{\n            if escaped {{ escaped = false; }} else if ch == '\\\\' {{ escaped = true; }} else if ch == '\"' {{ quoted = false; }}\n        }} else {{\n            match ch {{\n                '\"' => quoted = true,\n                '[' | '{{' => depth += 1,\n                ']' | '}}' => depth -= 1,\n                ',' if depth == 0 => return tail[..index].trim().to_string(),\n                _ => {{}}\n            }}\n        }}\n    }}\n    tail.trim().trim_end_matches('}}').trim().to_string()\n}}\n\nfn parse_i32(value: &str) -> i32 {{ value.trim().parse().unwrap_or_default() }}\nfn parse_f64(value: &str) -> f64 {{ value.trim().parse().unwrap_or_default() }}\nfn parse_string(value: &str) -> String {{ value.trim().trim_matches('\"').replace(\"\\\\\\\"\", \"\\\"\") }}\nfn parse_i32_vec(value: &str) -> Vec<i32> {{ value.trim().trim_matches(|ch| ch == '[' || ch == ']').split(',').filter(|item| !item.trim().is_empty()).map(parse_i32).collect() }}\nfn parse_string_vec(value: &str) -> Vec<String> {{ value.trim().trim_matches(|ch| ch == '[' || ch == ']').split(',').filter(|item| !item.trim().is_empty()).map(parse_string).collect() }}\n\nfn main() {{\n    let mut data = String::new();\n    if io::stdin().read_to_string(&mut data).is_err() {{ return; }}\n    {}\n    let result = Solution::{}({});\n    println!(\"{{:?}}\", result);\n}}\n",
            snippet.code,
            rust_bindings,
            rust_method,
            names.join(", ")
        ),
        "ruby" => format!(
            "require \"json\"\n\n{}\n\ndef solve(data)\n  params = JSON.parse(data)\n  {}({}).to_json\nend\n\nputs solve(STDIN.read)\n",
            snippet.code,
            problem.function_name,
            names
                .iter()
                .map(|name| format!("params[\"{name}\"]"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => return Err(format!("unsupported LeetCode language: {language_key}")),
    };
    Ok(code)
}

fn exported_name(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Value".to_string(),
    }
}

fn go_type(type_name: &str) -> &'static str {
    match type_name {
        "integer" => "int",
        "integer[]" => "[]int",
        "string" => "string",
        "string[]" => "[]string",
        "boolean" => "bool",
        "double" | "float" => "float64",
        _ => "json.RawMessage",
    }
}

fn rust_method_name(snippet: &str) -> Option<String> {
    let regex = regex::Regex::new(r"pub\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)").ok()?;
    regex
        .captures(snippet)
        .and_then(|capture| capture.get(1))
        .map(|name| name.as_str().to_string())
}

fn rust_binding(name: &str, type_name: &str) -> Result<String, String> {
    let field = format!("json_field(&data, \"{name}\")");
    let expression = match type_name {
        "integer" => format!("parse_i32(&{field})"),
        "integer[]" => format!("parse_i32_vec(&{field})"),
        "string" => format!("parse_string(&{field})"),
        "string[]" => format!("parse_string_vec(&{field})"),
        "boolean" => format!("{field}.trim() == \"true\""),
        "double" | "float" => format!("parse_f64(&{field})"),
        other => return Err(format!("mechanical Rust adapter does not support {other}")),
    };
    Ok(format!("let {name} = {expression};"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::leetcode_api::{LeetCodeCodeSnippet, LeetCodeParameter};

    fn two_sum_problem() -> LeetCodeProblem {
        LeetCodeProblem {
            frontend_id: "1".into(),
            title: "Two Sum".into(),
            title_slug: "two-sum".into(),
            difficulty: "Easy".into(),
            hints: Vec::new(),
            content: String::new(),
            function_name: "twoSum".into(),
            parameters: vec![
                LeetCodeParameter {
                    name: "nums".into(),
                    type_name: "integer[]".into(),
                },
                LeetCodeParameter {
                    name: "target".into(),
                    type_name: "integer".into(),
                },
            ],
            code_snippets: vec![LeetCodeCodeSnippet {
                lang: "JavaScript".into(),
                lang_slug: "javascript".into(),
                code: "var twoSum = function(nums, target) {\n    \n};".into(),
            }],
            example_testcase_list: Vec::new(),
        }
    }

    #[test]
    fn maps_supported_active_file_extensions() {
        assert_eq!(language_key_for_extension("py"), Some("python"));
        assert_eq!(language_key_for_extension("ts"), Some("typescript"));
        assert_eq!(language_key_for_extension("txt"), None);
    }

    #[test]
    fn javascript_adapter_keeps_official_snippet_and_adds_json_runner() {
        let code = adapt_snippet_mechanical(&two_sum_problem(), "javascript")
            .expect("javascript should adapt");
        assert!(code.contains("var twoSum = function"));
        assert!(code.contains("function solve(data)"));
        assert!(code.contains("twoSum(params.nums, params.target)"));
        assert!(code.contains("JSON.stringify(result)"));
    }

    #[test]
    fn go_adapter_decodes_named_parameters_and_calls_solution() {
        let mut problem = two_sum_problem();
        problem.code_snippets = vec![LeetCodeCodeSnippet {
            lang: "Go".into(),
            lang_slug: "golang".into(),
            code: "func twoSum(nums []int, target int) []int { return nil }".into(),
        }];
        let code = adapt_snippet_mechanical(&problem, "go").expect("go should adapt");
        assert!(code.contains("Nums []int `json:\"nums\"`"));
        assert!(code.contains("twoSum(params.Nums, params.Target)"));
        assert!(code.contains("json.NewEncoder(os.Stdout).Encode(result)"));
    }

    #[test]
    fn rust_adapter_declares_solution_and_calls_snippet_method() {
        let mut problem = two_sum_problem();
        problem.code_snippets = vec![LeetCodeCodeSnippet {
            lang: "Rust".into(),
            lang_slug: "rust".into(),
            code: "impl Solution { pub fn two_sum(nums: Vec<i32>, target: i32) -> Vec<i32> { vec![] } }".into(),
        }];
        let code = adapt_snippet_mechanical(&problem, "rust").expect("rust should adapt");
        assert!(code.contains("pub struct Solution;"));
        assert!(code.contains("Solution::two_sum(nums, target)"));
    }

    #[test]
    fn generated_go_and_rust_two_sum_sources_compile() {
        let mut problem = two_sum_problem();
        let root =
            std::env::temp_dir().join(format!("netherize_leetcode_adapter_{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create adapter temp dir");

        problem.code_snippets = vec![LeetCodeCodeSnippet {
            lang: "Go".into(),
            lang_slug: "golang".into(),
            code: "func twoSum(nums []int, target int) []int { return []int{} }".into(),
        }];
        let go_path = root.join("solution.go");
        std::fs::write(
            &go_path,
            adapt_snippet_mechanical(&problem, "go").expect("adapt go"),
        )
        .expect("write go source");
        if std::process::Command::new("go")
            .arg("version")
            .output()
            .is_ok()
        {
            let output = std::process::Command::new("go")
                .args(["build", "-o"])
                .arg(root.join("go-solution"))
                .arg(&go_path)
                .output()
                .expect("run go build");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        problem.code_snippets = vec![LeetCodeCodeSnippet {
            lang: "Rust".into(),
            lang_slug: "rust".into(),
            code: "impl Solution { pub fn two_sum(_nums: Vec<i32>, _target: i32) -> Vec<i32> { vec![] } }".into(),
        }];
        let rust_path = root.join("solution.rs");
        std::fs::write(
            &rust_path,
            adapt_snippet_mechanical(&problem, "rust").expect("adapt rust"),
        )
        .expect("write rust source");
        let output = std::process::Command::new("rustc")
            .arg(&rust_path)
            .arg("-o")
            .arg(root.join("rust-solution"))
            .output()
            .expect("run rustc");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
