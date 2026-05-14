cha_plugin_sdk::plugin!(ApiMisuse);

struct ApiMisuse;

fn loc(path: &str, line: u32) -> Location {
    Location {
        path: path.into(),
        start_line: line,
        start_col: 0,
        end_line: line,
        end_col: 0,
        name: None,
    }
}

fn finding(smell: &str, path: &str, line: u32, msg: &str, suggestion: &str) -> Finding {
    Finding {
        smell_name: smell.into(),
        category: SmellCategory::Dispensables,
        severity: Severity::Warning,
        location: loc(path, line),
        message: msg.into(),
        suggested_refactorings: vec![suggestion.into()],
        actual_value: None,
        threshold: None,
    }
}

fn err(smell: &str, path: &str, line: u32, msg: &str, suggestion: &str) -> Finding {
    Finding {
        smell_name: smell.into(),
        category: SmellCategory::Dispensables,
        severity: Severity::Error,
        location: loc(path, line),
        message: msg.into(),
        suggested_refactorings: vec![suggestion.into()],
        actual_value: None,
        threshold: None,
    }
}

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
            // Exclude chip names like "ESP32-S3"
            if i >= 5 && bytes[i - 5..i] == [0x45, 0x53, 0x50, 0x33, 0x32] {
                continue;
            }
            return true;
        }
    }
    false
}

fn has_spec_phase_id(line: &str) -> bool {
    // 0x50 0x68 0x61 0x73 0x65 0x20 <digit>
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

impl PluginImpl for ApiMisuse {
    fn name() -> String {
        "api-misuse".into()
    }

    fn analyze(input: AnalysisInput) -> Vec<Finding> {
        let mut findings = Vec::new();

        if input.language != "rust" {
            return findings;
        }

        for (line_no, line) in input.content.lines().enumerate() {
            let n = (line_no + 1) as u32;
            let t = line.trim();

            // --- Magic Fixed constants ---

            if t.contains("Fixed::from_raw(128)") && !t.contains("const ") {
                findings.push(finding(
                    "magic-fixed-half", &input.path, n,
                    "Use `Fixed::HALF` instead of `Fixed::from_raw(128)`",
                    "Replace with Fixed::HALF",
                ));
            }

            if t.contains("Fixed::from_raw(256)") && !t.contains("const ") {
                findings.push(finding(
                    "magic-fixed-one", &input.path, n,
                    "Use `Fixed::ONE` instead of `Fixed::from_raw(256)`",
                    "Replace with Fixed::ONE",
                ));
            }

            // --- API bypass (should use existing Rect/Point methods) ---

            if (t.contains("min_x") || t.contains("max_x"))
                && t.contains("q[")
                && (t.contains(".x") || t.contains(".y"))
            {
                findings.push(finding(
                    "manual-quad-bbox", &input.path, n,
                    "Use `Rect::bounding_quad()` instead of manual min/max",
                    "Replace loop with Rect::bounding_quad(&q)",
                ));
            }

            if t.contains(".x.to_int()") && t.contains(".y.to_int()") {
                findings.push(finding(
                    "point-floor", &input.path, n,
                    "Use `point.floor()` instead of `(p.x.to_int(), p.y.to_int())`",
                    "Replace with Point::floor()",
                ));
            }

            if t.contains(".x.floor()") && t.contains(".w).ceil()") {
                findings.push(finding(
                    "manual-pixel-bounds", &input.path, n,
                    "Use `Rect::pixel_bounds()` instead of manual floor/ceil",
                    "Replace with rect.pixel_bounds()",
                ));
            }

            // --- Internal spec ID leakage ---

            if t.starts_with("//") || t.starts_with("*") || t.starts_with("/*") {
                if has_spec_subtask_id(t) {
                    findings.push(err(
                        "spec-id-leak", &input.path, n,
                        "Internal spec subtask ID leaked into source comment",
                        "Remove or replace with human-readable description",
                    ));
                }
                if has_spec_phase_id(t) {
                    findings.push(err(
                        "spec-id-leak", &input.path, n,
                        "Internal spec phase ID leaked into source comment",
                        "Remove or replace with human-readable description",
                    ));
                }
            }

            // --- Stale naming (v0.9 renames) ---

            if t.contains("DrawBackend") && !t.contains("Canvas") && !t.contains("compose_backend") {
                findings.push(finding(
                    "stale-naming", &input.path, n,
                    "`DrawBackend` was renamed to `Canvas` in v0.9",
                    "Replace with Canvas",
                ));
            }

            if t.contains("SwDrawBackend") {
                findings.push(finding(
                    "stale-naming", &input.path, n,
                    "`SwDrawBackend` was renamed to `SwRenderer` in v0.9",
                    "Replace with SwRenderer",
                ));
            }

            if t.contains("mirui::backend") && !t.contains("mirui::surface") {
                findings.push(finding(
                    "stale-naming", &input.path, n,
                    "`mirui::backend` was moved to `mirui::surface` in v0.9",
                    "Replace with mirui::surface",
                ));
            }

            if t.contains("FramebufBackend") {
                findings.push(finding(
                    "stale-naming", &input.path, n,
                    "`FramebufBackend` was renamed to `FramebufSurface` in v0.9",
                    "Replace with FramebufSurface",
                ));
            }

            // --- UK spelling (project uses US) ---

            if t.contains("centre") && !t.contains("\"centre") {
                findings.push(finding(
                    "spelling-us", &input.path, n,
                    "Use `center` (US spelling) instead of `centre`",
                    "Replace centre with center",
                ));
            }

            // --- Fixed64 in per-pixel hot path (ESP perf killer) ---

            if t.contains("Fixed64::from_fixed") && t.contains("row.raw") {
                findings.push(finding(
                    "fixed64-hot-path", &input.path, n,
                    "Fixed64 in per-pixel row loop; costs ~10x on RV32 vs Fixed",
                    "Keep Fixed64 in per-quad prep only, use Fixed in per-pixel",
                ));
            }
        }

        findings
    }
}
