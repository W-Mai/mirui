use alloc::string::String;

pub(super) fn encode(mime: &str) -> String {
    let mime = mime.to_ascii_lowercase();
    match mime.as_str() {
        "text/plain" | "text/html" | "image/png" => mime,
        _ => alloc::format!("web {mime}"),
    }
}

pub(super) fn decode(mime: &str) -> &str {
    mime.strip_prefix("web ").unwrap_or(mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_mapping_preserves_standard_and_custom_representations() {
        for mime in ["text/plain", "text/html", "image/png"] {
            assert_eq!(encode(mime), mime);
            assert_eq!(decode(mime), mime);
        }
        for mime in ["application/x-mirui-pixels", "image/webp", "text/csv"] {
            assert_eq!(encode(mime), alloc::format!("web {mime}"));
            assert_eq!(decode(&encode(mime)), mime);
        }
        assert_eq!(encode("TEXT/PLAIN"), "text/plain");
        assert_eq!(encode("APPLICATION/X-Pixels"), "web application/x-pixels");
        assert_eq!(decode("application/web-example"), "application/web-example");
    }
}
