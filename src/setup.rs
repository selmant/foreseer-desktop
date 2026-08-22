const SETUP_HTML_TEMPLATE: &str = include_str!("setup.html");
const SETUP_EVENT_JS: &str = include_str!("setup-event.js");

pub fn get_setup_html(recovery_message: &str) -> String {
    let recovery = if recovery_message.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="status-box error" role="alert">{}</div>"#,
            escape_html(recovery_message)
        )
    };

    SETUP_HTML_TEMPLATE
        .replace("{{RECOVERY_MESSAGE}}", &recovery)
        .replace("{{SETUP_EVENT_JS}}", SETUP_EVENT_JS)
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::get_setup_html;

    #[test]
    fn setup_page_embeds_the_typed_protocol_listener() {
        let html = get_setup_html("");
        assert!(html.contains("foreseerSetupProtocolV1"));
        assert!(html.contains("detail.status"));
        assert!(html.contains("detail.message"));
        assert!(html.contains("foreseerNative"));
        assert!(html.contains("setup.standalone"));
        assert!(html.contains("Use Standalone"));
        assert!(!html.contains("{{SETUP_EVENT_JS}}"));
        assert!(!html.contains("{{RECOVERY_MESSAGE}}"));
        assert!(!html.contains("jelliumHost"));
    }

    #[test]
    fn setup_page_escapes_startup_recovery_errors() {
        let html = get_setup_html("Unable to start <script>alert('xss')</script>");

        assert!(html.contains("Unable to start &lt;script&gt;alert(&#39;xss&#39;)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert('xss')</script>"));
    }
}
