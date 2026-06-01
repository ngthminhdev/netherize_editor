use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    pub seq: u64,
    #[serde(rename = "type")]
    pub message_type: String, // "request", "response", or "event"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub seq: u64,
    #[serde(rename = "type")]
    pub message_type: String, // "request"
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub seq: u64,
    #[serde(rename = "type")]
    pub message_type: String, // "response"
    pub request_seq: u64,
    pub success: bool,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    #[serde(rename = "type")]
    pub message_type: String, // "event"
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

// ── Shared Protocol Objects ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub name: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    pub source: Option<Source>,
    pub line: i64,
    pub column: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    pub name: String,
    #[serde(rename = "variablesReference")]
    pub variables_reference: i64,
    pub expensive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    #[serde(rename = "variablesReference")]
    pub variables_reference: i64,
    pub evaluate_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub id: Option<i64>,
    pub verified: bool,
    pub line: Option<i64>,
    pub message: Option<String>,
}

// ── Specific Event Bodies ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoppedEventBody {
    pub reason: String,
    #[serde(rename = "threadId")]
    pub thread_id: Option<i64>,
    #[serde(rename = "allThreadsStopped")]
    pub all_threads_stopped: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEventBody {
    pub category: Option<String>,
    pub output: String,
}

// ── Specific Request Arguments ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeRequestArguments {
    #[serde(rename = "clientID")]
    pub client_id: String,
    #[serde(rename = "clientName")]
    pub client_name: String,
    #[serde(rename = "adapterID")]
    pub adapter_id: String,
    #[serde(rename = "linesStartAt1")]
    pub lines_start_at_1: bool,
    #[serde(rename = "columnsStartAt1")]
    pub columns_start_at_1: bool,
    #[serde(rename = "pathFormat")]
    pub path_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceBreakpoint {
    pub line: i64,
    pub column: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetBreakpointsArguments {
    pub source: Source,
    pub breakpoints: Vec<SourceBreakpoint>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_with_type_field() {
        let req = Request {
            seq: 1,
            message_type: "request".to_string(),
            command: "initialize".to_string(),
            arguments: Some(serde_json::json!({"adapterID": "flutter"})),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"request""#));
        assert!(json.contains(r#""command":"initialize""#));
        assert!(json.contains(r#""adapterID":"flutter""#));
    }

    #[test]
    fn request_serializes_without_arguments_when_none() {
        let req = Request {
            seq: 2,
            message_type: "request".to_string(),
            command: "configurationDone".to_string(),
            arguments: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("arguments"));
    }

    #[test]
    fn response_deserializes_from_json() {
        let json = r#"{
            "seq": 5,
            "type": "response",
            "request_seq": 1,
            "success": true,
            "command": "initialize",
            "body": {"supportsConfigurationDoneRequest": true}
        }"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.seq, 5);
        assert_eq!(resp.request_seq, 1);
        assert!(resp.success);
        assert_eq!(resp.command, "initialize");
        assert!(resp.body.is_some());
    }

    #[test]
    fn response_handles_failure_with_message() {
        let json = r#"{
            "seq": 6,
            "type": "response",
            "request_seq": 2,
            "success": false,
            "command": "launch",
            "message": "Failed to launch"
        }"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert!(!resp.success);
        assert_eq!(resp.message.as_deref(), Some("Failed to launch"));
    }

    #[test]
    fn event_deserializes_stopped_event() {
        let json = r#"{
            "seq": 10,
            "type": "event",
            "event": "stopped",
            "body": {
                "reason": "breakpoint",
                "threadId": 1,
                "allThreadsStopped": true
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert_eq!(event.event, "stopped");
        let body: StoppedEventBody = serde_json::from_value(event.body.unwrap()).unwrap();
        assert_eq!(body.reason, "breakpoint");
        assert_eq!(body.thread_id, Some(1));
        assert_eq!(body.all_threads_stopped, Some(true));
    }

    #[test]
    fn event_deserializes_output_event() {
        let json = r#"{
            "seq": 11,
            "type": "event",
            "event": "output",
            "body": {
                "category": "stdout",
                "output": "Hello, world!\n"
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        let body: OutputEventBody = serde_json::from_value(event.body.unwrap()).unwrap();
        assert_eq!(body.category.as_deref(), Some("stdout"));
        assert_eq!(body.output, "Hello, world!\n");
    }

    #[test]
    fn event_deserializes_terminated_event() {
        let json = r#"{
            "seq": 12,
            "type": "event",
            "event": "terminated"
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert_eq!(event.event, "terminated");
        assert!(event.body.is_none());
    }

    #[test]
    fn thread_serializes_and_deserializes() {
        let thread = Thread {
            id: 1,
            name: "main".to_string(),
        };
        let json = serde_json::to_string(&thread).unwrap();
        let deserialized: Thread = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, 1);
        assert_eq!(deserialized.name, "main");
    }

    #[test]
    fn stack_frame_with_source_roundtrips() {
        let frame = StackFrame {
            id: 100,
            name: "main".to_string(),
            source: Some(Source {
                name: Some("main.dart".to_string()),
                path: Some("/lib/main.dart".to_string()),
            }),
            line: 42,
            column: 8,
        };
        let json = serde_json::to_string(&frame).unwrap();
        let deserialized: StackFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, 100);
        assert_eq!(deserialized.line, 42);
        assert_eq!(deserialized.column, 8);
        assert_eq!(deserialized.source.as_ref().unwrap().name.as_deref(), Some("main.dart"));
    }

    #[test]
    fn stack_frame_without_source_roundtrips() {
        let frame = StackFrame {
            id: 200,
            name: "<anonymous>".to_string(),
            source: None,
            line: 1,
            column: 1,
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("null") || !json.contains("source"));
        let deserialized: StackFrame = serde_json::from_str(&json).unwrap();
        assert!(deserialized.source.is_none());
    }

    #[test]
    fn scope_serializes_with_variables_reference() {
        let scope = Scope {
            name: "Local".to_string(),
            variables_reference: 1001,
            expensive: false,
        };
        let json = serde_json::to_string(&scope).unwrap();
        assert!(json.contains(r#""variablesReference":1001"#));
        let deserialized: Scope = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "Local");
        assert_eq!(deserialized.variables_reference, 1001);
    }

    #[test]
    fn variable_with_nested_reference() {
        let var = Variable {
            name: "myList".to_string(),
            value: "List(3)".to_string(),
            variables_reference: 2001,
            evaluate_name: Some("myList".to_string()),
        };
        let json = serde_json::to_string(&var).unwrap();
        let deserialized: Variable = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "myList");
        assert_eq!(deserialized.variables_reference, 2001);
        assert_eq!(deserialized.evaluate_name.as_deref(), Some("myList"));
    }

    #[test]
    fn variable_without_evaluate_name() {
        let var = Variable {
            name: "x".to_string(),
            value: "42".to_string(),
            variables_reference: 0,
            evaluate_name: None,
        };
        let json = serde_json::to_string(&var).unwrap();
        let deserialized: Variable = serde_json::from_str(&json).unwrap();
        assert!(deserialized.evaluate_name.is_none());
    }

    #[test]
    fn breakpoint_verified_with_line() {
        let json = r#"{"id": 5, "verified": true, "line": 10}"#;
        let bp: Breakpoint = serde_json::from_str(json).unwrap();
        assert_eq!(bp.id, Some(5));
        assert!(bp.verified);
        assert_eq!(bp.line, Some(10));
    }

    #[test]
    fn breakpoint_unverified_with_message() {
        let json = r#"{"verified": false, "message": "Breakpoint not verified"}"#;
        let bp: Breakpoint = serde_json::from_str(json).unwrap();
        assert!(!bp.verified);
        assert!(bp.id.is_none());
        assert_eq!(bp.message.as_deref(), Some("Breakpoint not verified"));
    }

    #[test]
    fn source_breakpoint_with_column() {
        let sbp = SourceBreakpoint {
            line: 10,
            column: Some(5),
        };
        let json = serde_json::to_string(&sbp).unwrap();
        assert!(json.contains(r#""line":10"#));
        assert!(json.contains(r#""column":5"#));
    }

    #[test]
    fn source_breakpoint_without_column() {
        let sbp = SourceBreakpoint {
            line: 20,
            column: None,
        };
        let json = serde_json::to_string(&sbp).unwrap();
        assert!(json.contains(r#""line":20"#));
    }

    #[test]
    fn set_breakpoints_arguments_roundtrips() {
        let args = SetBreakpointsArguments {
            source: Source {
                name: Some("main.dart".to_string()),
                path: Some("/lib/main.dart".to_string()),
            },
            breakpoints: vec![
                SourceBreakpoint { line: 10, column: None },
                SourceBreakpoint { line: 20, column: Some(5) },
            ],
        };
        let json = serde_json::to_string(&args).unwrap();
        let deserialized: SetBreakpointsArguments = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.breakpoints.len(), 2);
        assert_eq!(deserialized.source.path.as_deref(), Some("/lib/main.dart"));
    }

    #[test]
    fn initialize_request_arguments_roundtrips() {
        let args = InitializeRequestArguments {
            client_id: "netherize".to_string(),
            client_name: "Netherize Editor".to_string(),
            adapter_id: "flutter".to_string(),
            lines_start_at_1: true,
            columns_start_at_1: true,
            path_format: "path".to_string(),
        };
        let json = serde_json::to_string(&args).unwrap();
        assert!(json.contains(r#""clientID":"netherize""#));
        assert!(json.contains(r#""adapterID":"flutter""#));
        assert!(json.contains(r#""linesStartAt1":true"#));
        let deserialized: InitializeRequestArguments = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.adapter_id, "flutter");
    }

    #[test]
    fn stopped_event_body_defaults_thread_id_to_none() {
        let json = r#"{"reason": "step"}"#;
        let body: StoppedEventBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.reason, "step");
        assert!(body.thread_id.is_none());
    }

    #[test]
    fn output_event_body_with_no_category() {
        let json = r#"{"output": "raw output"}"#;
        let body: OutputEventBody = serde_json::from_str(json).unwrap();
        assert!(body.category.is_none());
        assert_eq!(body.output, "raw output");
    }
}
