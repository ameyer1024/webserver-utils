use pulldown_cmark::CowStr;



pub mod markdown;

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

pub fn seeded_rng(seed: &str) -> impl rand::Rng {
    rand_seeder::Seeder::from(seed).make_rng::<rand_pcg::Pcg64>()
}

pub fn default_offset() -> time::UtcOffset {
    // TODO: use tz-rs or something to get the right timezone?
    time::UtcOffset::UTC
}

fn short_month(month: time::Month) -> &'static str {
    match month {
        time::Month::January => "Jan",
        time::Month::February => "Feb",
        time::Month::March => "Mar",
        time::Month::April => "Apr",
        time::Month::May => "May",
        time::Month::June => "Jun",
        time::Month::July => "Jul",
        time::Month::August => "Aug",
        time::Month::September => "Sep",
        time::Month::October => "Oct",
        time::Month::November => "Nov",
        time::Month::December => "Dec",
    }
}

#[derive(Default)]
pub enum DateFmt {
    #[default]
    Short,
    Shorter,
}

pub fn format_date(date: &time::OffsetDateTime, offset: Option<time::UtcOffset>, mode: DateFmt) -> String {
    let offset = offset.unwrap_or_else(default_offset);
    let current = time::OffsetDateTime::now_utc().to_offset(offset);
    let current_year = current.year();
    let date = date.to_offset(offset);

    let year = date.year();
    let month = short_month(date.month());
    let day = date.day();
    let hour = date.hour();
    let minute = date.minute();

    match mode {
        DateFmt::Short => {
            if year == current_year {
                format!("{month} {day:02}, {hour:02}:{minute:02}")
            } else {
                format!("{month} {day:02} {year:04}, {hour:02}:{minute:02}")
            }
        },
        DateFmt::Shorter => {
            if year == current_year {
                format!("{month} {day:02}")
            } else {
                format!("{month} {day:02} {year:04}")
            }
        }
    }
}

#[derive(Default)]
pub enum DurationFmt {
    #[default]
    Shorter,
    Short,
    Long,
}

pub fn format_age(date: &time::OffsetDateTime, fmt: DurationFmt) -> String {
    use std::fmt::Write;

    let dur = time::OffsetDateTime::now_utc() - *date;

    // TODO: weeks/months, or just shift back to date after a point
    let weeks = dur.whole_weeks();
    let days = dur.whole_days();
    let hours = dur.whole_hours() % 24;
    let minutes = dur.whole_minutes() % 60;
    let seconds = dur.as_seconds_f64() % 60.0;
    let mut out = String::new();

    match fmt {
        DurationFmt::Shorter => {
            if days > 31 {
                return format_date(date, None, DateFmt::Shorter);
            } else if weeks > 0 {
                write!(out, "{}w", weeks).unwrap();
            } else if days > 0 {
                write!(out, "{}d", days).unwrap();
            } else if hours > 0 {
                write!(out, "{}h", hours).unwrap();
            } else if minutes > 0 {
                write!(out, "{}m", minutes).unwrap();
            } else {
                let seconds = seconds.round() as i64;
                write!(out, "{}s", seconds).unwrap();
            }
            write!(out, " ago").unwrap();
        },
        DurationFmt::Short => {
            if weeks > 0 {
                write!(out, "{} week{}", weeks, plural(weeks)).unwrap();
            } else if days > 0 {
                write!(out, "{} day{}", days, plural(days)).unwrap();
            } else if hours > 0 {
                write!(out, "{} hour{}", hours, plural(hours)).unwrap();
            } else if minutes > 0 {
                write!(out, "{} min{}", minutes, plural(minutes)).unwrap();
            } else {
                let seconds = seconds.round() as i64;
                write!(out, "{} sec{}", seconds, plural(seconds)).unwrap();
            }
            write!(out, " ago").unwrap();
        },
        DurationFmt::Long => {
            if days > 0 { write!(out, "{} day{} ", days, plural(days)).unwrap(); }
            if hours > 0 { write!(out, "{} hour{} ", hours, plural(hours)).unwrap(); }
            if minutes > 0 { write!(out, "{} minute{} ", minutes, plural(minutes)).unwrap(); }
            if days <= 0 && hours <= 0 && (seconds > 0.0 || minutes <= 0) {
                write!(out, "{seconds:.3} seconds ").unwrap();
            }
            write!(out, "ago").unwrap();
        }
    }
    out
}

fn plural(number: i64) -> &'static str {
    if number != 1 { "s" } else { "" }
}
