
pub fn sanitize_html(html: &str) -> String {
    // ammonia::clean(html)
    ammonia::Builder::default()
        .add_tags(["i"])
        .add_tag_attributes("i", ["part", "tag"])
        .add_generic_attribute_prefixes(["data-"])
        .clean(html)
        .to_string()
}
pub fn sanitize_html_trusted(html: &str) -> String {
    // be *very* trusting about allowed tags
    let mut tags = std::collections::HashSet::new();
    let mut scan = html;
    while let Some(start) = scan.find("</") {
        scan = &scan[start + 2..];
        if let Some(end) = scan.find(">") {
            if end < 32 {
                tags.insert(&scan[..end]);
            }
        }
    }
    // println!("Found tags: {:?}", tags);

    // TODO: whitelist all "lang-" classes for code blocks (by searching for them in input?)

    ammonia::Builder::default()
        .add_tags(["i"])
        .add_tag_attributes("i", ["part", "tag"])
        .add_tag_attributes("h1", ["id"])
        .add_tag_attributes("h2", ["id"])
        .add_tag_attributes("h3", ["id"])
        .add_tag_attributes("h4", ["id"])
        .add_tag_attributes("h5", ["id"])
        .add_tag_attributes("h6", ["id"])
        .add_generic_attribute_prefixes(["data-"])
        .add_generic_attributes(["style", "aria-labelledby", "aria-label", "aria-role"])
        .add_allowed_classes("a", ["heading-anchor", "heading-anchor-inner"])
        .add_allowed_classes("div", ["footnote-definition", "heading-wrapper", "h1", "h2", "h3", "h4", "h5", "h6"])
        .add_allowed_classes("sup", ["footnote-definition-label", "footnote-definition-reference"])
        .clean_content_tags([].into())
        .add_tags(["script", "style", "template", "slot"])
        .add_tag_attributes("template", ["shadowrootmode", "name", "mode"])
        .add_tags(tags)
        .clean(html)
        .to_string()
}

pub fn sanitize_html_to_text(text: &str) -> String {
    ammonia::Builder::empty().clean(text).to_string()
}
pub fn sanitize_text(text: &str) -> String {
    ammonia::clean_text(text)
}
/// Works in any space other than an unquoted attribute (why do those exist...)
pub fn sanitize_body_text(text: &str) -> String {
    // Inlined from ammonia's clean_text, and simplified
    // TODO: determine average size change to avoid realloc
    let mut ret_val = String::with_capacity(usize::max(4, text.len()));
    for c in text.chars() {
        let replacement = match c {
            // this character, when confronted, will start a tag
            '<' => "&lt;",
            // in an unquoted attribute, will end the attribute value
            '>' => "&gt;",
            // in an attribute surrounded by double quotes, this character will end the attribute value
            '\"' => "&quot;",
            // in an attribute surrounded by single quotes, this character will end the attribute value
            '\'' => "&apos;",
            // starts an entity reference
            '&' => "&amp;",
            // a spec-compliant browser will perform this replacement anyway, but the middleware might not
            '\0' => "&#65533;",
            // ALL OTHER CHARACTERS ARE PASSED THROUGH VERBATIM
            _ => {
                ret_val.push(c);
                continue;
            }
        };
        ret_val.push_str(replacement);
    }
    ret_val
}
pub fn percent_encode(text: &str) -> String {
    const FRAGMENT: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
        .add(b' ').add(b'\'').add(b'"')
        .add(b'&')
        .add(b'[').add(b']').add(b'\\')
        .add(b'{').add(b'}').add(b'|')
        .add(b'<').add(b'>').add(b'`');

    percent_encoding::utf8_percent_encode(text, FRAGMENT).to_string()
}

pub fn sanitize_link(link: &str) -> String {
    // TODO: verify this
    let parsed_link = ammonia::Url::parse(&link).ok();
    let link = parsed_link.and_then(|parsed_link| {
        match parsed_link.scheme() {
            "http" | "https" => Some(parsed_link.to_string()),
            _ => None
        }
    }).unwrap_or_else(|| {
        percent_encode(&format!("data:text/html;charset=utf8,<h1>Invalid link: <code>{}</code></h1>", sanitize_text(&link)))
    });
    link
}
