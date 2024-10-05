
#[macro_use]
extern crate tracing;

use std::borrow::Cow;
use std::collections::HashMap;

use scraper::{Html, Selector};
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Default)]
pub enum EmbedState {
    #[default]
    Normal,
    Loading,
    Failed,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Info {
    #[cfg_attr(feature = "serde", serde(default))]
    pub state: EmbedState,
    pub base_url: String,
    pub url: String,
    pub site: String,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub opengraph: OpenGraph,
    pub meta: HashMap<String, String>,
    pub feeds: Vec<(String, String)>,
    pub icons: Vec<Icon>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub cached_icon: Option<Icon>,
    pub links: Vec<Link>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
pub struct Icon {
    pub kind: IconKind,
    pub url: String,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub size: Option<String>,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none", rename="type"))]
    pub type_: Option<String>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy)]
pub enum IconKind {
    Favicon,
    TouchIcon,
    MaskIcon,
    DefaultFavicon,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LinkKind {
    Link,
    Anchor,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Link {
    pub kind: LinkKind,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub rel: Option<String>,
    pub href: Option<String>,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub text: Option<String>,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub additional: Option<HashMap<String, String>>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Default)]
pub struct OpenGraph {
    pub type_: Option<String>,
    pub properties: HashMap<String, String>, // TODO: this erases duplicate values
    pub objects: HashMap<Cow<'static, str>, Vec<OpenGraphObj>>,
}
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OpenGraphObj {
    pub url: Option<String>,
    // secure_url: Option<String>,
    // type_: Option<String>,
    // alt: Option<String>,
    // width: Option<u32>,
    // height: Option<u32>,
    pub properties: HashMap<String, String>,
}

impl Info {
    pub fn unknown(url: &url::Url) -> Self {
        let mut icons = Vec::new();

        if let Some(default) = url.join("/favicon.ico").ok() {
            icons.push(Icon {
                kind: IconKind::DefaultFavicon,
                url: default.into(),
                type_: None,
                size: None,
            });
        }

        Self {
            state: EmbedState::Failed,
            base_url: url.to_string(),
            url: url.to_string(),
            site: url.domain().unwrap_or("").into(),
            lang: None,
            title: None,
            description: None,
            opengraph: OpenGraph::default(),
            meta: Default::default(),
            feeds: Default::default(),
            icons,
            cached_icon: None,
            links: Default::default(),
        }
    }
    pub fn blank_embed(url: &str) -> Info {
        let parsed_url = url::Url::parse(url).ok();
        let site = parsed_url.as_ref().and_then(|u| u.domain());
        let mut icons = Vec::new();

        if let Some(default) = parsed_url.as_ref().and_then(|u| u.join("/favicon.ico").ok()) {
            icons.push(Icon {
                kind: IconKind::DefaultFavicon,
                url: default.into(),
                type_: None,
                size: None,
            });
        }

        Info {
            state: EmbedState::Normal,
            base_url: url.to_string(),
            url: url.to_string(),
            site: site.unwrap_or("").into(),
            lang: None,
            title: None,
            description: None,
            opengraph: OpenGraph::default(),
            meta: Default::default(),
            feeds: Default::default(),
            icons,
            cached_icon: None,
            links: Default::default(),
        }
    }
    pub fn direct_image(url: &url::Url, content_type: Option<&str>) -> Self {
        let mut this = Self::unknown(url);
        this.state = EmbedState::Normal;
        let filename = url.path_segments().into_iter().flatten().last();
        if let Some(filename) = filename {
            let bytes = percent_encoding::percent_decode_str(&filename).collect::<Vec<_>>();
            if let Ok(filename) = std::str::from_utf8(&bytes) {
                this.title = Some(filename.into());
            }
        }
        this.opengraph.type_ = Some("website".into());
        this.opengraph.objects.entry("image".into()).or_default()
            .push(OpenGraphObj {
                url: Some(url.to_string()),
                properties: [].into_iter()
                    .chain(if let Some(t) = content_type { Some(("type".into(), t.into())) } else { None })
                    .collect(),
            });
        this
    }
    pub fn best_icon(&self, target: u32, target_range: std::ops::Range<u32>) -> Option<&Icon> {
        if let Some(icon) = &self.cached_icon {
            return Some(icon);
        }

        // eprintln!("Selecting icon, {} {:?}", target, target_range);
        let res = self.icons.iter().enumerate().max_by_key(|(i, icon)| {
            let is_mask = matches!(icon.kind, IconKind::MaskIcon);

            let size;
            if let Some(s) = icon.size.as_ref()
                .and_then(|s| s.split("x").next())
                .and_then(|i| i.parse::<u32>().ok())
            {
                size = s;
            } else {
                size = match icon.kind {
                    IconKind::DefaultFavicon => 0,
                    IconKind::Favicon => 0,
                    IconKind::TouchIcon => 180,
                    IconKind::MaskIcon => 32,
                };
            };

            let is_svg = icon.type_.as_deref() == Some("image/svg+xml");

            let index = if matches!(icon.kind, IconKind::DefaultFavicon) { -1 } else { *i as i32 };
            let diff = if size < target { -4096 } else { target as i32 - size as i32 };

            let score = (!is_mask, is_svg, target_range.contains(&size), diff, index);
            // eprintln!("    {:?} {:?} {:?} {:?}: {:?}", icon.url, icon.kind, icon.size, icon.type_, score);
            score
        }).map(|(_, icon)| icon);
        // eprintln!("Selected icon {:?}", res);
        res
    }
}

impl Icon {
    pub fn blank() -> Self {
        Icon {
            url: "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg'/>".into(),
            kind: IconKind::Favicon,
            size: None,
            type_: None,
        }
    }
}

impl OpenGraph {
    fn process_prop(&mut self, name: &str, value: &str) {
        // TODO: handle locale array
        match name {
            "type" => self.type_ = Some(value.into()),
            "image" => self.objects.entry("image".into())
                .or_default().push(OpenGraphObj { url: Some(value.into()), properties: Default::default() }),
            "video" => self.objects.entry("video".into())
                .or_default().push(OpenGraphObj { url: Some(value.into()), properties: Default::default() }),
            "audio" => self.objects.entry("audio".into())
                .or_default().push(OpenGraphObj { url: Some(value.into()), properties: Default::default() }),
            _ => {
                if let Some((scope, prop)) = name.split_once(':') {
                    let list = self.objects.entry(scope.to_owned().into()).or_default();
                    let last = if let Some(l)= list.last_mut() { l } else {
                        list.push(OpenGraphObj { url: None, properties: Default::default() });
                        list.last_mut().unwrap()
                    };
                    if prop == "url" {
                        last.url = Some(value.into());
                    } else {
                        last.properties.insert(prop.into(), value.into());
                    }
                } else {
                    self.properties.insert(name.into(), value.into());
                }
            }
        }
    }
}

// TODO: properly resolve relative URLs
pub fn parse_document(bytes: &[u8], content_type: Option<&str>, url: &url::Url) -> Info {

    match content_type {
        Some("text/html") => (),
        Some(s) if s.starts_with("image") => {
            return Info::direct_image(url, Some(s));
        }
        _ => {
            warn!("Unknown content type: {:?}", content_type);
            return Info::unknown(url);
        }
    }

    // TODO: support non-utf8 pages;
    // let (cow, encoding_used, had_errors) = encoding_rs::SHIFT_JIS.decode(bytes);
    // assert_eq!(&cow[..], expectation);
    // assert_eq!(encoding_used, SHIFT_JIS);
    // assert!(!had_errors);
    let s = String::from_utf8_lossy(bytes);
    let s = &*s;

    let document = Html::parse_document(s);

    let lang_sel = Selector::parse("body[lang], html[lang]").unwrap();
    let mut lang = None;
    for elem in document.select(&lang_sel) {
        lang = elem.attr("lang").map(ToOwned::to_owned);
    }

    let title_sel = Selector::parse("title").unwrap();
    let mut title = None;
    if let Some(elem) = document.select(&title_sel).next() {
        title = Some(normalize_whitespace_text(elem.text(), Some(256)));
    }

    let mut description = None;
    let mut meta = std::collections::HashMap::new();
    let mut opengraph = OpenGraph {
        type_: None,
        properties: Default::default(),
        objects: Default::default(),
    };

    let base_sel = Selector::parse("base[href]").unwrap();
    let base_url = document.select(&base_sel).next()
        .and_then(|e| e.attr("href"))
        .and_then(|u| url.join(u).ok()) // TODO: log errors?
        .unwrap_or_else(|| url.clone());

    let meta_tags = Selector::parse("meta").unwrap();
    for tag in document.select(&meta_tags) {
        if let Some(charset) = tag.attr("charset") {
            // TODO: ???
        } else if let Some(equiv) = tag.attr("http-equiv") {
            let content = tag.attr("content");
            // redirects?

        } else if let (Some(name), Some(value)) = (tag.attr("name").or_else(|| tag.attr("property")), tag.attr("content")) {
            // why does open graph protocol use a non-standard HTML attr?  `name` exists and works fine...


            if name.starts_with("og:") {
                opengraph.process_prop(&name["og:".len() ..], value);
            }

            // https://developer.mozilla.org/en-US/docs/Web/HTML/Element/meta/name
            meta.insert(name.into(), value.into());
            match name {
                "description" => {
                    description = Some(value.into());
                },
                "author" => (),
                "keywords" => (),
                "theme-color" => (),
                "color-scheme" => (),
                "robots" => (),
                _ => (),
            }
        }
    }

    let mut links = Vec::new();
    let mut feeds = Vec::new();
    let mut icons = Vec::new();

    let link_tags = Selector::parse("link").unwrap();
    for tag in document.select(&link_tags) {
        // https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes/rel
        if let Some(rel) = tag.attr("rel") {
            let elems = rel.split(' ').collect::<Vec<_>>();

            if elems.contains(&"stylesheet") || elems.contains(&"preload") {
                continue;
            }

            let resolved_href: Option<String> = tag.attr("href")
                .map(|u| base_url.join(u).map(Into::into).unwrap_or_else(|_| u.into()));

            let icon_kind = match rel {
                "icon" => Some(IconKind::Favicon),
                "shortcut icon" => Some(IconKind::Favicon),
                "mask-icon" => Some(IconKind::MaskIcon),
                "apple-touch-icon" => Some(IconKind::TouchIcon),
                _ => None,
            };
            if let (Some(kind), Some(href)) = (icon_kind, &resolved_href) {
                icons.push(Icon {
                    kind: kind,
                    url: href.clone(),
                    size: tag.attr("sizes").map(Into::into),
                    type_: tag.attr("type").map(Into::into),
                });
            }

            match rel {
                "canonical" => (),

                "me" => (),

                "author" => (),
                "license" => (),
                "help" => (),
                "prev" => (),
                _ => (),
            }

            // if rel == "canonical" {
            //     html.set_url(get_attribute(attrs, "href"));
            // } else
            if rel == "alternate" {
                match tag.attr("type") {
                    Some(type_ @ ("application/atom+xml" | "application/json" | "application/rdf+xml" | "application/rss+xml" | "application/xml" | "text/xml")) => {
                        if let Some(href) = &resolved_href {
                            feeds.push((href.into(), type_.into()));
                        }
                    },
                    _ => (),
                }
            }

            let rest = tag.value().attrs()
                .filter(|(k, _)| !matches!(*k, "href" | "ref"))
                .map(|(k, v)| (k.into(), v.into())).collect::<HashMap<String, String>>();

            links.push(Link {
                kind: LinkKind::Link,
                rel: Some(rel.into()),
                href: resolved_href,
                text: None,
                additional: if rest.is_empty() { None } else { Some(rest) }
            });
        }
    }

    let anchor_tags = Selector::parse("a").unwrap();
    for tag in document.select(&anchor_tags) {
        // https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes/rel
        let rest = tag.value().attrs()
            .filter(|(k, _)| !matches!(*k, "href" | "ref"))
            .map(|(k, v)| (k.into(), v.into())).collect::<HashMap<String, String>>();

        let resolved_href: Option<String> = tag.attr("href")
            .map(|u| base_url.join(u).map(Into::into).unwrap_or_else(|_| u.into()));

        if let Some(rel) = tag.attr("rel") {
            // let elems = rel.split(' ');
            match rel {
                "alternate" => (), // often RSS/atom, look at type and href
                "me" => (),
                "author" => (),
                "license" => (),
                "help" => (),
                "prev" => (),
                _ => (),
            }

            if rel == "alternate" {
                match tag.attr("type") {
                    Some(type_ @ ("application/atom+xml" | "application/json" | "application/rdf+xml" | "application/rss+xml" | "application/xml" | "text/xml")) => {
                        if let Some(href) = &resolved_href {
                            feeds.push((href.into(), type_.into()));
                        }
                    },
                    _ => (),
                }
            }
        }


        links.push(Link {
            kind: LinkKind::Anchor,
            rel: tag.attr("rel").map(Into::into),
            href: resolved_href,
            text: Some(normalize_whitespace_text(tag.text(), Some(256))),
            additional: if rest.is_empty() { None } else { Some(rest) }
        });
    }

    // if icons.is_empty() {
        if let Some(default) = base_url.join("/favicon.ico").ok() {
            icons.push(Icon {
                kind: IconKind::DefaultFavicon,
                url: default.into(),
                type_: None,
                size: None,
            });
        }
    // }

    if description.is_none() {
        if let Some(desc) = opengraph.properties.get("description") {
            description = Some(desc.clone());
        }
    }
    if title.is_none() {
        if let Some(t) = opengraph.properties.get("title") {
            title = Some(t.clone());
        }
    }

    if description.is_none() && url.domain().map(|s| s.ends_with("wikipedia.org") || s.ends_with("wikipedia.org.")).unwrap_or(false) {
        let wikipedia_shortdesc = Selector::parse("div.shortdescription, #shared-image-desc #fileinfotpl_desc + td .description.en").unwrap();
        let wikipedia_first_para = Selector::parse("#mw-content-text > div > p:first-of-type").unwrap();
        if let Some(tag) = document.select(&wikipedia_shortdesc).next() {
            description = Some(normalize_whitespace_text(tag.text(), Some(256)));
        } else if let Some(tag) = document.select(&wikipedia_first_para).next() {
            description = Some(normalize_whitespace_text(tag.text(), Some(256)));
        }
    }

    // TODO: classify "best" icons
    // - easier to use size classes, and apple-touch-icon
    // TODO: check icons support?

    // TODO: publish date / edit dates?


    // TODO: Twitter metadata (https://developer.twitter.com/en/docs/twitter-for-websites/cards/overview/markup)


    // TODO: mastodon rel="me" links?

    // TODO: better understanding of opengraph, https://en.rakko.tools/tools/9/
    // TODO: JSON-ld? https://json-ld.org/
    // TODO: microformats? https://microformats.org/


    Info {
        state: EmbedState::Normal,
        base_url: base_url.into(),
        url: url.to_string(),
        site: url.domain().unwrap_or("").into(),
        lang,
        title,
        description,
        meta,
        opengraph,
        feeds,
        icons,
        cached_icon: None,
        links,
    }
}

/// Join a series of strings, merging whitespace into single spaces;
/// optionally truncates at a given length.
fn normalize_whitespace_text<'a>(text: impl Iterator<Item = &'a str>, max: Option<usize>) -> String {
    let mut out = String::new();

    let mut ended_with_whitespace = true;
    for mut seg in text {
        if ended_with_whitespace {
            seg = seg.trim_start();
        }
        out.reserve(seg.len());

        let mut in_whitespace = false;
        for c in seg.chars() {
            if c.is_whitespace() {
                if !in_whitespace {
                    out.push(' ');
                    in_whitespace = true;
                }
            } else {
                out.push(c);
                in_whitespace = false;
            }
        }
        if max.map(|n| out.len() >= n).unwrap_or(false) {
            break;
        }
        ended_with_whitespace = in_whitespace;
    }

    let mut len = out.trim_end().len(); // trim tailing whitespace
    if let Some(max) = max {
        len = len.min(max);
    }
    out.truncate(len);
    out
}
