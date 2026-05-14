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

            if t.starts_with("//") || t.starts_with("/*") || t.starts_with("*") {
                continue;
            }

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
        }

        findings
    }
}
