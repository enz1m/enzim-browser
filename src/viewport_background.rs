pub(crate) fn parse(expected: Option<&str>, message: &str) -> Option<iced::Color> {
    #[derive(serde::Deserialize)]
    struct Background {
        url: String,
        color: [u8; 4],
    }
    if message.len() > 4096 {
        return None;
    }
    let data: Background = serde_json::from_str(message).ok()?;
    if expected != Some(data.url.as_str()) || data.color[3] != 255 {
        return None;
    }
    Some(iced::Color::from_rgb8(
        data.color[0],
        data.color[1],
        data.color[2],
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn viewport_background_accepts_only_current_document_and_opaque_bounded_colors() {
        let url = Some("https://example.org/");
        assert_eq!(
            super::parse(
                url,
                r#"{"url":"https://example.org/","color":[19,26,33,255]}"#
            ),
            Some(iced::Color::from_rgb8(19, 26, 33)),
            "current document background is accepted"
        );
        for payload in [
            r#"{"url":"https://old.example/","color":[19,26,33,255]}"#,
            r#"{"url":"https://example.org/","color":[19,26,33,0]}"#,
            r#"{"url":"https://example.org/","color":[256,26,33,255]}"#,
            r#"{"url":"https://example.org/","color":[19,26,33]}"#,
        ] {
            assert_eq!(
                super::parse(url, payload),
                None,
                "invalid or stale background: {payload}"
            );
        }
        assert_eq!(
            super::parse(
                None,
                r#"{"url":"https://example.org/","color":[19,26,33,255]}"#
            ),
            None,
            "a background without a current document is refused"
        );
    }
}
