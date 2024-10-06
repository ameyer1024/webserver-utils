
#[macro_use]
extern crate tracing;

pub mod coawait;
pub mod markdown;
pub mod render;
pub mod sanitize;

use pulldown_cmark::CowStr;

struct FoundLink {
    label: String,
    title: String,
    url: String,
}

pub fn process_markdown(text: &str, base_url: Option<&str>) -> (String, Vec<String>) {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(
        pulldown_cmark::Options::empty()
        | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
        | pulldown_cmark::Options::ENABLE_TABLES
        | pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION
        | pulldown_cmark::Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES
        | pulldown_cmark::Options::ENABLE_MATH
        | pulldown_cmark::Options::ENABLE_GFM
        | pulldown_cmark::Options::ENABLE_TASKLISTS
    );
    fn ensure_signature<F: for<'a> Fn(pulldown_cmark::BrokenLink<'a>) -> Option<(CowStr<'a>, CowStr<'a>)>>(f: F) -> F { f }
    let callback = ensure_signature(|l: pulldown_cmark::BrokenLink| {
        // debug!("broken link: {:?}", l);
        // debug!("Raw text: {:?}", &text[l.span.clone()]);
        // debug!("Before: {:?}, after: {:?}", text[..l.span.start].chars().last(), text[l.span.end..].chars().next());

        // Manually check for wiki-style links by looking at range in text
        // TODO: escaping, partial wiki links?
        let wiki_link = matches!((text[..l.span.start].chars().last(), text[l.span.end..].chars().next()), (Some('['), Some(']')));

        match l.link_type {
            // [name][reference]
            pulldown_cmark::LinkType::Reference => (),
            // [name][]
            pulldown_cmark::LinkType::Collapsed => (),
            // [reference]
            pulldown_cmark::LinkType::Shortcut => (),
            _ => return None,
        }

        let mut discovered_link = None;

        // TODO: better way to rewrite link text for shortcuts?
        if wiki_link {
            // TODO: look up page title
            // TODO: remove wrapping brackets?
            let path = &*l.reference;
            discovered_link = Some(FoundLink {
                label: path.into(),
                title: path.into(),
                url: path.into(),
            });
        } else if l.reference.starts_with("rust:") {
            let path = &l.reference["rust:".len()..];
            // TODO: this only allows adding title text, not rewriting link text
            discovered_link = Some(FoundLink {
                label: path.into(),
                title: path.into(),
                // This has an unfortunate chain of redirects,
                // but fixing that requires reimplementing this manually
                url: format!("https://docs.rs/{path}?go_to_first=true").into(),
            });
        } else if false {
            // TODO: search for local referencable pages (wikilinks)
        }

        if let Some(res) = discovered_link {
            if matches!(l.link_type, pulldown_cmark::LinkType::Shortcut) {
                // TODO: escaping ## in label
                Some((format!("##{}##{}", res.label, res.url).into(), res.title.into()))
            } else {
                Some((res.url.into(), res.title.into()))
            }
        } else {
            None
        }
    });
    let parser = pulldown_cmark::Parser::new_with_broken_link_callback(text, options, Some(callback));

    let mut metadata = Default::default();
    let iter = markdown::metadata_extractor(parser, &mut metadata);
    let mut html_output = String::new();
    markdown::push_html(&mut html_output, iter, markdown::Options {
        soft_breaks_as_hard: false,
        heading_generate_ids: true,
        heading_anchor_links: markdown::HeadingAnchorMode::HeadingIsLink,
        embeds: true,
        base_url: base_url.map(Into::into),
    });
    (html_output, metadata)
}
