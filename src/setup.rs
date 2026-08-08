const SETUP_HTML_TEMPLATE: &str = include_str!("setup.html");
const SETUP_EVENT_JS: &str = include_str!("setup-event.js");

pub fn get_setup_html(api_base: &str) -> String {
    SETUP_HTML_TEMPLATE
        .replace("{{API_BASE}}", api_base)
        .replace("{{SETUP_EVENT_JS}}", SETUP_EVENT_JS)
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
        assert!(!html.contains("{{SETUP_EVENT_JS}}"));
    }
}
