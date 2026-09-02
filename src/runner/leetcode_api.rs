use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeetCodeCodeSnippet {
    pub lang: String,
    #[serde(rename = "langSlug")]
    pub lang_slug: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeetCodeParameter {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeetCodeTestCase {
    pub input: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeetCodeProblem {
    pub frontend_id: String,
    pub title: String,
    pub title_slug: String,
    pub content: String,
    pub function_name: String,
    pub parameters: Vec<LeetCodeParameter>,
    pub code_snippets: Vec<LeetCodeCodeSnippet>,
    pub example_testcase_list: Vec<String>,
    /// "Easy" / "Medium" / "Hard" as LeetCode reports it (may be empty).
    pub difficulty: String,
    /// LeetCode's own hints, in order; shown on demand in the Dojo panel.
    pub hints: Vec<String>,
}

pub fn normalize_problem_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) && !trimmed.is_empty() {
        return Ok(trimmed.to_string());
    }
    let candidate = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let url = url::Url::parse(trimmed).map_err(|err| format!("invalid LeetCode URL: {err}"))?;
        let mut segments = url
            .path_segments()
            .ok_or_else(|| "LeetCode URL has no path".to_string())?;
        if segments.next() != Some("problems") {
            return Err("URL must point to leetcode.com/problems/<slug>".to_string());
        }
        segments
            .next()
            .ok_or_else(|| "LeetCode URL is missing a problem slug".to_string())?
            .to_string()
    } else {
        trimmed.to_string()
    };
    if candidate.is_empty()
        || candidate.starts_with('-')
        || candidate.ends_with('-')
        || !candidate
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        return Err("enter a problem ID, lowercase title slug, or LeetCode URL".to_string());
    }
    Ok(candidate)
}

pub fn parse_metadata(raw: &str) -> Result<(String, Vec<LeetCodeParameter>), String> {
    #[derive(Deserialize)]
    struct Metadata {
        name: String,
        #[serde(default)]
        params: Vec<MetadataParam>,
    }
    #[derive(Deserialize)]
    struct MetadataParam {
        name: String,
        #[serde(rename = "type")]
        type_name: String,
    }
    let metadata: Metadata =
        serde_json::from_str(raw).map_err(|err| format!("invalid problem metadata: {err}"))?;
    if metadata.name.trim().is_empty() {
        return Err("problem metadata has no function name".to_string());
    }
    Ok((
        metadata.name,
        metadata
            .params
            .into_iter()
            .map(|param| LeetCodeParameter {
                name: param.name,
                type_name: param.type_name,
            })
            .collect(),
    ))
}

pub fn extract_test_cases(problem: &LeetCodeProblem) -> Vec<LeetCodeTestCase> {
    let outputs = extract_example_outputs(&problem.content);
    problem
        .example_testcase_list
        .iter()
        .enumerate()
        .map(|(index, example)| {
            let values: Vec<Value> = example
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| {
                    serde_json::from_str(line).unwrap_or_else(|_| Value::String(line.into()))
                })
                .collect();
            let mut input = Map::new();
            for (parameter, value) in problem.parameters.iter().zip(values) {
                input.insert(parameter.name.clone(), value);
            }
            LeetCodeTestCase {
                input: Value::Object(input).to_string(),
                expected: outputs.get(index).cloned().unwrap_or_default(),
            }
        })
        .collect()
}

fn extract_example_outputs(content: &str) -> Vec<String> {
    let Ok(pre_re) = regex::Regex::new(r"(?is)<pre[^>]*>(.*?)</pre>") else {
        return Vec::new();
    };
    let Ok(output_re) = regex::Regex::new(
        r"(?is)(?:<strong>\s*)?Output\s*:(?:\s*</strong>)?\s*(.*?)(?:\n|<br\s*/?>|$)",
    ) else {
        return Vec::new();
    };
    let Ok(tag_re) = regex::Regex::new(r"(?is)<[^>]+>") else {
        return Vec::new();
    };
    pre_re
        .captures_iter(content)
        .filter_map(|pre| output_re.captures(pre.get(1)?.as_str()))
        .filter_map(|capture| capture.get(1))
        .map(|value| {
            tag_re
                .replace_all(value.as_str(), "")
                .replace("&quot;", "\"")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&")
                .trim()
                .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_slug_and_leetcode_urls() {
        assert_eq!(normalize_problem_input("two-sum").as_deref(), Ok("two-sum"));
        assert_eq!(
            normalize_problem_input("https://leetcode.com/problems/two-sum/description/")
                .as_deref(),
            Ok("two-sum")
        );
        assert!(normalize_problem_input("not a slug").is_err());
    }

    #[test]
    fn preserves_numeric_problem_id_for_async_resolution() {
        assert_eq!(normalize_problem_input(" 15 ").as_deref(), Ok("15"));
    }

    #[test]
    fn parses_function_name_and_parameter_metadata() {
        let raw = r#"{"name":"twoSum","params":[{"name":"nums","type":"integer[]"},{"name":"target","type":"integer"}]}"#;
        let (name, params) = parse_metadata(raw).expect("metadata should parse");
        assert_eq!(name, "twoSum");
        assert_eq!(params[0].name, "nums");
        assert_eq!(params[1].type_name, "integer");
    }

    #[test]
    fn combines_example_inputs_with_html_outputs() {
        let problem = LeetCodeProblem {
            frontend_id: "1".into(),
            title: "Two Sum".into(),
            title_slug: "two-sum".into(),
            difficulty: "Easy".into(),
            hints: Vec::new(),
            content: r#"<pre><strong>Input:</strong> nums = [2,7,11,15], target = 9
<strong>Output:</strong> [0,1]</pre>"#
                .into(),
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
            code_snippets: Vec::new(),
            example_testcase_list: vec!["[2,7,11,15]\n9".into()],
        };
        assert_eq!(
            extract_test_cases(&problem),
            vec![LeetCodeTestCase {
                input: r#"{"nums":[2,7,11,15],"target":9}"#.into(),
                expected: "[0,1]".into(),
            }]
        );
    }
}
