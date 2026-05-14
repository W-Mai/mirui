cha_plugin_sdk::plugin!(ApiMisuse);

struct ApiMisuse;

fn finding(
    smell: &str,
    sev: Severity,
    path: &str,
    line: u32,
    col: u32,
    msg: &str,
    suggestion: &str,
) -> Finding {
    Finding {
        smell_name: smell.into(),
        category: SmellCategory::Dispensables,
        severity: sev,
        location: Location {
            path: path.into(),
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: 0,
            name: None,
        },
        message: msg.into(),
        suggested_refactorings: vec![suggestion.into()],
        actual_value: None,
        threshold: None,
    }
}

impl PluginImpl for ApiMisuse {
    fn name() -> String {
        "api-misuse".into()
    }

    fn smells() -> Vec<String> {
        vec![
            "magic-fixed-half".into(),
            "magic-fixed-one".into(),
            "manual-quad-bbox".into(),
            "point-floor".into(),
            "manual-pixel-bounds".into(),
            "spec-id-leak".into(),
            "stale-naming".into(),
            "spelling-us".into(),
            "fixed64-hot-path".into(),
        ]
    }

    fn analyze(input: AnalysisInput) -> Vec<Finding> {
        let mut findings = Vec::new();

        if input.language != "rust" {
            return findings;
        }

        // Skip test files for most rules
        let is_test = input.role == FileRole::Test;

        // --- Rule: spec-id-leak (via parsed comments, not raw text) ---
        for comment in &input.comments {
            let t = &comment.text;
            if has_spec_subtask_id(t) || has_spec_phase_id(t) {
                findings.push(finding(
                    "spec-id-leak",
                    Severity::Error,
                    &input.path,
                    comment.line,
                    0,
                    "Internal spec subtask/phase ID leaked into comment",
                    "Remove or replace with human-readable description",
                ));
            }
        }

        if is_test {
            return findings;
        }

        // --- Rule: magic-fixed-half / magic-fixed-one (via tree-sitter) ---
        let magic_matches = tree_query::run_query(
            "(call_expression
                function: (scoped_identifier) @fn
                arguments: (arguments (integer_literal) @val)
            )"
        );
        for m in &magic_matches {
            let fn_name = m.iter().find(|c| c.capture_name == "fn");
            let val = m.iter().find(|c| c.capture_name == "val");
            if let (Some(f), Some(v)) = (fn_name, val) {
                if f.text == "Fixed::from_raw" {
                    if v.text == "128" {
                        findings.push(finding(
                            "magic-fixed-half",
                            Severity::Warning,
                            &input.path,
                            v.start_line,
                            v.start_col,
                            "Use `Fixed::HALF` instead of `Fixed::from_raw(128)`",
                            "Replace with Fixed::HALF",
                        ));
                    } else if v.text == "256" {
                        findings.push(finding(
                            "magic-fixed-one",
                            Severity::Warning,
                            &input.path,
                            v.start_line,
                            v.start_col,
                            "Use `Fixed::ONE` instead of `Fixed::from_raw(256)`",
                            "Replace with Fixed::ONE",
                        ));
                    }
                }
            }
        }

        // --- Rule: stale-naming (via imports) ---
        for imp in &input.imports {
            let s = &imp.source;
            if s.contains("DrawBackend") && !s.contains("Canvas") {
                findings.push(finding(
                    "stale-naming",
                    Severity::Warning,
                    &input.path,
                    imp.line,
                    imp.col,
                    "`DrawBackend` was renamed to `Canvas` in v0.9",
                    "Replace with Canvas",
                ));
            }
            if s.contains("SwDrawBackend") {
                findings.push(finding(
                    "stale-naming",
                    Severity::Warning,
                    &input.path,
                    imp.line,
                    imp.col,
                    "`SwDrawBackend` was renamed to `SwRenderer` in v0.9",
                    "Replace with SwRenderer",
                ));
            }
            if s.contains("mirui::backend") {
                findings.push(finding(
                    "stale-naming",
                    Severity::Warning,
                    &input.path,
                    imp.line,
                    imp.col,
                    "`mirui::backend` was moved to `mirui::surface` in v0.9",
                    "Replace with mirui::surface",
                ));
            }
        }

        // --- Rule: spelling-us (via tree-sitter string search) ---
        let centre_matches = tree_query::run_query(
            "(identifier) @id"
        );
        for m in &centre_matches {
            for capture in m {
                if capture.text.contains("centre") {
                    findings.push(finding(
                        "spelling-us",
                        Severity::Warning,
                        &input.path,
                        capture.start_line,
                        capture.start_col,
                        "Use `center` (US spelling) instead of `centre`",
                        "Replace centre with center",
                    ));
                }
            }
        }

        // --- Rule: fixed64-hot-path (Fixed64 used inside for/loop body) ---
        let loop_matches = tree_query::run_query(
            "(for_expression body: (block) @body)"
        );
        for m in &loop_matches {
            for capture in m {
                if capture.capture_name == "body" {
                    // Check if this loop body contains Fixed64 usage
                    let body_nodes = tree_query::nodes_in_range(
                        capture.start_line,
                        capture.end_line,
                    );
                    for node in &body_nodes {
                        if node.text.contains("Fixed64") && node.text.contains("from_fixed") {
                            findings.push(finding(
                                "fixed64-hot-path",
                                Severity::Warning,
                                &input.path,
                                node.start_line,
                                node.start_col,
                                "Fixed64 in per-pixel loop; costs ~10x on RV32 vs Fixed",
                                "Keep Fixed64 in per-quad prep only",
                            ));
                            break;
                        }
                    }
                }
            }
        }

        findings
    }
}

// --- Spec ID detection (hex-only, no ASCII literals that could self-trigger) ---

fn has_spec_subtask_id(line: &str) -> bool {
    let bytes = line.as_bytes();
    // Pattern: 0x20 0x53 <0x30..0x39> (0x20 | 0x3A | EOL)
    for i in 0..bytes.len().saturating_sub(3) {
        if bytes[i] == 0x20
            && bytes[i + 1] == 0x53
            && bytes[i + 2] >= 0x30
            && bytes[i + 2] <= 0x39
            && (i + 3 >= bytes.len() || bytes[i + 3] == 0x20 || bytes[i + 3] == 0x3A)
        {
            if i >= 5 && bytes[i - 5..i] == [0x45, 0x53, 0x50, 0x33, 0x32] {
                continue;
            }
            return true;
        }
    }
    false
}

fn has_spec_phase_id(line: &str) -> bool {
    let needle: [u8; 6] = [0x50, 0x68, 0x61, 0x73, 0x65, 0x20];
    let bytes = line.as_bytes();
    for i in 0..bytes.len().saturating_sub(needle.len() + 1) {
        if bytes[i..i + needle.len()] == needle
            && bytes[i + needle.len()] >= 0x30
            && bytes[i + needle.len()] <= 0x39
        {
            return true;
        }
    }
    false
}
