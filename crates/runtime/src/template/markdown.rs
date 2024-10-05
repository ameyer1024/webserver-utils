
// From https://github.com/raphlinus/pulldown-cmark
// A modified version of the HTML writer that currently:
// - converts soft breaks into hard breaks

/*
The MIT License

Copyright 2015 Google Inc. All rights reserved.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
*/

use std::collections::HashMap;
use std::io::Write;

use pulldown_cmark_escape::{escape_href, escape_html, escape_html_body_text, StrWrite, IoWriter};
use pulldown_cmark::{BlockQuoteKind, CowStr};
use pulldown_cmark::Event;
use pulldown_cmark::{Alignment, CodeBlockKind, LinkType, Tag, TagEnd};

enum TableState {
    Head,
    Body,
}

pub fn metadata_extractor<'a: 'b, 'b>(iter: impl Iterator<Item = Event<'a>> + 'b, metadata_blocks: &'b mut Vec<String>) -> impl Iterator<Item = Event<'a>> + 'b {
    use next_gen::generator_fn::CallBoxed;
    #[next_gen::generator(yield(Event<'a>))]
    fn metadata_extractor_inner<'a, 'b>(mut iter: impl Iterator<Item = Event<'a>>, metadata_blocks: &'b mut Vec<String>) {
        while let Some(elem) = iter.next() {
            match elem {
                Event::Start(Tag::MetadataBlock(_)) => {
                    let mut text = String::new();
                    while let Some(elem) = iter.next() {
                        match elem {
                            Event::End(TagEnd::MetadataBlock(_)) => break,
                            Event::Text(t) => text.push_str(&*t),
                            _ => unimplemented!()
                        }
                    }
                    metadata_blocks.push(text);
                },
                _ => yield_!(elem),
            }
        }
    }
    metadata_extractor_inner.call_boxed((iter, metadata_blocks))
}

#[derive(Copy, Clone, Debug)]
pub enum HeadingAnchorMode {
    /// Headers do not include any links to themselves
    None,
    /// Makes the entire heading into a link
    /// * (+) Generally works in screen readers
    /// * (+) Works in most reader modes
    /// * (+) Larger tap target on mobile
    /// * (-) Can no longer select text in heading
    /// * (-) Can't include other links in heading
    HeadingIsLink,
    /// Adds a wrapper div and puts a link as a sibling following the heading
    /// * (+) Okay screen-reader support
    /// * (-) Adds hidden text to page ("Section link")
    /// * (-) Headings with < 13 characters disappear from Firefox's reader view
    LinkAfterHeading,
    /// Adds a link as the last child of the heading
    /// * (-) Becomes part of title in screen reader
    /// * (-) Becomes part of title in reader view
    /// * (-) Adds hidden text to page ("Section link")
    LinkInHeading,
}

#[derive(Clone)]
pub struct Options {
    pub soft_breaks_as_hard: bool,
    pub heading_generate_ids: bool,
    pub embeds: bool,
    pub heading_anchor_links: HeadingAnchorMode,
    pub base_url: Option<String>,
}
impl Default for Options {
    fn default() -> Self {
        Options {
            soft_breaks_as_hard: false,
            heading_generate_ids: false,
            embeds: false,
            heading_anchor_links: HeadingAnchorMode::None,
            base_url: None,
        }
    }
}

#[derive(Default)]
struct HeadingBuffer<'a> {
    text_buffer: String,
    active: bool,
    id: Option<CowStr<'a>>,
    classes: Vec<CowStr<'a>>,
    attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
}

#[derive(Debug)]
struct LinkIntercept<'a> {
    dest_url: CowStr<'a>,
    title: CowStr<'a>,
    #[allow(unused)]
    id: CowStr<'a>,
}

struct InterceptWriter<W> {
    inner: W,
    // buffer: String,
    // intercept: bool,
    intercept: Vec<String>,
}

#[derive(Debug)]
enum InterceptError<E> {
    Inner(E),
    Fmt(std::fmt::Error),
}
impl InterceptError<std::io::Error> {
    fn flatten(self) -> std::io::Error {
        match self {
            InterceptError::Inner(err) => err,
            InterceptError::Fmt(std::fmt::Error) => std::io::ErrorKind::Other.into(),
        }
    }
}
impl<W> StrWrite for InterceptWriter<W> where W: StrWrite {
    type Error = InterceptError<W::Error>;
    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        if let Some(buf) = self.intercept.last_mut() {
            buf.push_str(s);
            Ok(())
        } else {
            self.inner.write_str(s).map_err(InterceptError::Inner)
        }
    }
    fn write_fmt(&mut self, args: std::fmt::Arguments) -> Result<(), Self::Error> {
        if let Some(buf) = self.intercept.last_mut() {
            buf.write_fmt(args).map_err(InterceptError::Fmt)
        } else {
            self.inner.write_fmt(args).map_err(InterceptError::Inner)
        }
    }
}

struct HtmlWriter<'a, I, W> {
    /// Iterator supplying events.
    iter: I,

    /// Writer to write to.
    writer: InterceptWriter<W>,

    /// Whether or not the last write wrote a newline.
    end_newline: bool,

    /// Whether if inside a metadata block (text should not be written)
    in_non_writing_block: bool,

    cur_heading_id: Option<CowStr<'a>>,
    heading_buffer: HeadingBuffer<'a>,
    link_buffer: Option<LinkIntercept<'a>>,

    options: Options,

    base_url: Option<String>,

    table_state: TableState,
    table_alignments: Vec<Alignment>,
    table_cell_index: usize,
    numbers: HashMap<CowStr<'a>, usize>,
}

impl<'a, I, W> HtmlWriter<'a, I, W>
where
    I: Iterator<Item = Event<'a>>,
    W: StrWrite,
{
    fn new(iter: I, writer: W, options: Options) -> Self {
        assert!(options.base_url.as_ref().map(|s| s.ends_with("/")).unwrap_or(true));
        Self {
            iter,
            writer: InterceptWriter { inner: writer, intercept: Vec::new() },
            base_url: options.base_url.clone(),
            // base_url: options.base_url.as_ref()
            //     .map(|u| url::Url::parse("http://_").unwrap()
            //         .join(u).unwrap()),
            options,
            end_newline: true,
            in_non_writing_block: false,
            cur_heading_id: None,
            heading_buffer: Default::default(),
            link_buffer: None,
            table_state: TableState::Head,
            table_alignments: vec![],
            table_cell_index: 0,
            numbers: HashMap::new(),
        }
    }

    /// Writes a new line.
    fn write_newline(&mut self) -> Result<(), InterceptError<W::Error>> {
        self.end_newline = true;
        self.writer.write_str("\n")?;
        Ok(())
    }

    /// Writes a buffer, and tracks whether or not a newline was written.
    #[inline]
    fn write(&mut self, s: &str) -> Result<(), InterceptError<W::Error>> {
        self.writer.write_str(s)?;

        if !s.is_empty() {
            self.end_newline = s.ends_with('\n');
        }
        Ok(())
    }

    fn run(mut self) -> Result<(), InterceptError<W::Error>> {
        while let Some(event) = self.iter.next() {
            match event {
                Event::Start(tag) => {
                    self.start_tag(tag)?;
                }
                Event::End(tag) => {
                    self.end_tag(tag)?;
                }
                Event::Text(text) => {
                    if self.heading_buffer.active {
                        self.heading_buffer.text_buffer.push_str(&*text);
                    }
                    if !self.in_non_writing_block {
                        escape_html_body_text(&mut self.writer, &text)?;
                        self.end_newline = text.ends_with('\n');
                    }
                }
                Event::Code(text) => {
                    self.write("<code>")?;
                    if self.heading_buffer.active {
                        self.heading_buffer.text_buffer.push_str(&*text);
                    }
                    escape_html_body_text(&mut self.writer, &text)?;
                    self.write("</code>")?;
                }
                ref event @ (Event::InlineMath(ref text) | Event::DisplayMath(ref text)) => {
                    let mode = if matches!(event, Event::InlineMath(..)) { "$" } else { "$$" };
                    // TODO: actual math-mode handling
                    // (KaTeX / client javacript, or something serverside?  Could bake a svg sheet and cache it)
                    self.write("<code>")?;
                    self.write(mode)?;
                    if self.heading_buffer.active {
                        self.heading_buffer.text_buffer.push_str(&*text);
                    }
                    escape_html_body_text(&mut self.writer, &text)?;
                    self.write(mode)?;
                    self.write("</code>")?;
                }
                Event::Html(html) | Event::InlineHtml(html) => {
                    self.write(&html)?;
                }
                Event::SoftBreak => {
                    if self.heading_buffer.active {
                        self.heading_buffer.text_buffer.push('\n');
                    }
                    match self.options.soft_breaks_as_hard {
                        false => self.write_newline()?,
                        true  => self.write("<br />\n")?,
                    }
                }
                Event::HardBreak => {
                    if self.heading_buffer.active {
                        self.heading_buffer.text_buffer.push('\n');
                    }
                    self.write("<br />\n")?;
                }
                Event::Rule => {
                    if self.end_newline {
                        self.write("<hr />\n")?;
                    } else {
                        self.write("\n<hr />\n")?;
                    }
                }
                Event::FootnoteReference(name) => {
                    let len = self.numbers.len() + 1;
                    self.write("<sup class=\"footnote-reference\"><a href=\"#")?;
                    escape_html(&mut self.writer, &name)?;
                    self.write("\">")?;
                    let number = *self.numbers.entry(name).or_insert(len);
                    write!(&mut self.writer, "{}", number)?;
                    self.write("</a></sup>")?;
                }
                Event::TaskListMarker(true) => {
                    self.write("<span class=\"task-list-mark\">[x]</span> ")?;
                }
                Event::TaskListMarker(false) => {
                    self.write("<span class=\"task-list-mark\">[ ]</span> ")?;
                }
            }
        }
        Ok(())
    }

    fn write_heading_start(
        &mut self,
        level: pulldown_cmark::HeadingLevel,
        id: Option<CowStr<'a>>,
        classes: Vec<CowStr<'a>>,
        attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>
    ) -> Result<(), InterceptError<W::Error>> {

        match self.options.heading_anchor_links {
            HeadingAnchorMode::None => (),
            HeadingAnchorMode::HeadingIsLink => (),
            HeadingAnchorMode::LinkAfterHeading => {
                self.write("<div class=\"heading-wrapper ")?;
                write!(&mut self.writer, "{}", level)?;
                self.write("\">")?;
            },
            HeadingAnchorMode::LinkInHeading => (),
        }

        self.write("<")?;
        write!(&mut self.writer, "{}", level)?;

        if let Some(id) = &id {
            self.write(" id=\"")?;
            escape_html(&mut self.writer, &id)?;
            self.write("\"")?;
        }
        let mut classes = classes.iter();
        if let Some(class) = classes.next() {
            self.write(" class=\"")?;
            escape_html(&mut self.writer, class)?;
            for class in classes {
                self.write(" ")?;
                escape_html(&mut self.writer, class)?;
            }
            self.write("\"")?;
        }
        for (attr, value) in attrs {
            self.write(" ")?;
            escape_html(&mut self.writer, &attr)?;
            if let Some(val) = value {
                self.write("=\"")?;
                escape_html(&mut self.writer, &val)?;
                self.write("\"")?;
            } else {
                self.write("=\"\"")?;
            }
        }
        self.write(">")?;

        match self.options.heading_anchor_links {
            HeadingAnchorMode::None => (),
            HeadingAnchorMode::HeadingIsLink => {
                if let Some(id) = &id {
                    self.write("<a href=\"#")?;
                    escape_href(&mut self.writer, &id)?;
                    self.write("\" class=\"heading-anchor-inner\">")?;
                }
            },
            HeadingAnchorMode::LinkAfterHeading => (),
            HeadingAnchorMode::LinkInHeading => (),
        }

        if let Some(id) = id {
            self.cur_heading_id = Some(id);
        }

        Ok(())
    }

    /// Writes the start of an HTML tag.
    fn start_tag(&mut self, tag: Tag<'a>) -> Result<(), InterceptError<W::Error>> {
        match tag {
            Tag::HtmlBlock => Ok(()),
            Tag::Paragraph => {
                if self.end_newline {
                    self.write("<p>")
                } else {
                    self.write("\n<p>")
                }
            }
            Tag::Heading {
                level,
                id,
                classes,
                attrs,
            } => {
                if self.end_newline {
                    self.end_newline = false;
                } else {
                    self.write("\n")?;
                }

                if self.options.heading_generate_ids && id.is_none()
                    || matches!(self.options.heading_anchor_links, HeadingAnchorMode::None)
                {
                    self.writer.intercept.push(String::with_capacity(64));
                    self.heading_buffer.active = true;
                    self.heading_buffer.id = id;
                    self.heading_buffer.classes = classes;
                    self.heading_buffer.attrs = attrs;
                } else {
                    self.write_heading_start(level, id, classes, attrs)?;
                }
                Ok(())
            }
            Tag::Table(alignments) => {
                self.table_alignments = alignments;
                self.write("<table>")
            }
            Tag::TableHead => {
                self.table_state = TableState::Head;
                self.table_cell_index = 0;
                self.write("<thead><tr>")
            }
            Tag::TableRow => {
                self.table_cell_index = 0;
                self.write("<tr>")
            }
            Tag::TableCell => {
                match self.table_state {
                    TableState::Head => {
                        self.write("<th")?;
                    }
                    TableState::Body => {
                        self.write("<td")?;
                    }
                }
                match self.table_alignments.get(self.table_cell_index) {
                    Some(&Alignment::Left) => self.write(" style=\"text-align: left\">"),
                    Some(&Alignment::Center) => self.write(" style=\"text-align: center\">"),
                    Some(&Alignment::Right) => self.write(" style=\"text-align: right\">"),
                    _ => self.write(">"),
                }
            }
            Tag::BlockQuote(kind) => {
                // TODO: fork pulldown-cmark, generalize this?
                let class_str = match kind {
                    None => "",
                    Some(kind) => match kind {
                        BlockQuoteKind::Note => " class=\"markdown-alert-note\"",
                        BlockQuoteKind::Tip => " class=\"markdown-alert-tip\"",
                        BlockQuoteKind::Important => " class=\"markdown-alert-important\"",
                        BlockQuoteKind::Warning => " class=\"markdown-alert-warning\"",
                        BlockQuoteKind::Caution => " class=\"markdown-alert-caution\"",
                    },
                };
                if !self.end_newline {
                    self.write("\n")?;
                }
                self.write("<blockquote")?;
                self.write(class_str)?;
                self.write(">\n")
            }
            Tag::CodeBlock(info) => {
                if !self.end_newline {
                    self.write_newline()?;
                }
                match info {
                    CodeBlockKind::Fenced(info) => {
                        let lang = info.split(' ').next().unwrap();
                        if lang.is_empty() {
                            self.write("<pre><code>")
                        } else {
                            self.write("<pre><code class=\"language-")?;
                            escape_html(&mut self.writer, lang)?;
                            self.write("\">")
                        }
                    }
                    CodeBlockKind::Indented => self.write("<pre><code>"),
                }
            }
            Tag::List(Some(1)) => {
                if self.end_newline {
                    self.write("<ol>\n")
                } else {
                    self.write("\n<ol>\n")
                }
            }
            Tag::List(Some(start)) => {
                if self.end_newline {
                    self.write("<ol start=\"")?;
                } else {
                    self.write("\n<ol start=\"")?;
                }
                write!(&mut self.writer, "{}", start)?;
                self.write("\">\n")
            }
            Tag::List(None) => {
                if self.end_newline {
                    self.write("<ul>\n")
                } else {
                    self.write("\n<ul>\n")
                }
            }
            Tag::Item => {
                if self.end_newline {
                    self.write("<li>")
                } else {
                    self.write("\n<li>")
                }
            }
            Tag::Emphasis => self.write("<em>"),
            Tag::Strong => self.write("<strong>"),
            Tag::Strikethrough => self.write("<del>"),
            Tag::Link {
                link_type: LinkType::Email,
                dest_url,
                title,
                id: _,
            } => {
                self.write("<a href=\"mailto:")?;
                escape_href(&mut self.writer, &dest_url)?;
                if !title.is_empty() {
                    self.write("\" title=\"")?;
                    escape_html(&mut self.writer, &title)?;
                }
                self.write("\">")
            }
            Tag::Link {
                link_type: _,
                dest_url,
                title,
                id,
            } => {
                if self.options.embeds {
                    self.link_buffer = Some(LinkIntercept {
                        dest_url,
                        title,
                        id,
                    });
                    self.writer.intercept.push(String::with_capacity(256));
                    Ok(())
                } else {
                    self.write("<a href=\"")?;
                    let href = self.handle_url(&*dest_url)?;
                    escape_href(&mut self.writer, &href)?;
                    if !title.is_empty() {
                        self.write("\" title=\"")?;
                        escape_html(&mut self.writer, &title)?;
                    }
                    self.write("\">")
                }
            }
            Tag::Image {
                link_type: _,
                dest_url,
                title,
                id: _,
            } => {
                self.write("<img src=\"")?;
                let href = self.handle_url(&*dest_url)?;
                escape_href(&mut self.writer, &href)?;
                self.write("\" alt=\"")?;
                self.raw_text()?;
                if !title.is_empty() {
                    self.write("\" title=\"")?;
                    escape_html(&mut self.writer, &title)?;
                }
                self.write("\" />")
            }
            Tag::FootnoteDefinition(name) => {
                if self.end_newline {
                    self.write("<div class=\"footnote-definition\" id=\"")?;
                } else {
                    self.write("\n<div class=\"footnote-definition\" id=\"")?;
                }
                escape_html(&mut self.writer, &name)?;
                self.write("\"><sup class=\"footnote-definition-label\">")?;
                let len = self.numbers.len() + 1;
                let number = *self.numbers.entry(name).or_insert(len);
                write!(&mut self.writer, "{}", number)?;
                self.write("</sup>")
            }
            Tag::MetadataBlock(_) => {
                self.in_non_writing_block = true;
                Ok(())
            }
        }
    }

    fn end_tag(&mut self, tag: TagEnd) -> Result<(), InterceptError<W::Error>> {
        match tag {
            TagEnd::HtmlBlock => {}
            TagEnd::Paragraph => {
                self.write("</p>\n")?;
            }
            TagEnd::Heading(level) => {
                if self.heading_buffer.active {
                    self.heading_buffer.active = false;

                    let content = self.writer.intercept.pop().unwrap();

                    let mut id = self.heading_buffer.id.take();
                    if id.is_none() {
                        let string = std::mem::take(&mut self.heading_buffer.text_buffer);

                        let string = string.to_lowercase().chars().filter_map(|c| match c {
                            c if c.is_whitespace() => Some('-'),
                            _ => Some(c),
                        }).collect::<String>();

                        id = Some(string.into());
                    }

                    let classes = std::mem::take(&mut self.heading_buffer.classes);
                    let attrs = std::mem::take(&mut self.heading_buffer.attrs);
                    self.write_heading_start(level, id, classes, attrs)?;

                    self.write(&content)?;
                }

                let id = self.cur_heading_id.take();

                match self.options.heading_anchor_links {
                    HeadingAnchorMode::None => (),
                    HeadingAnchorMode::HeadingIsLink => {
                        if let Some(_) = &id {
                            self.write("</a>")?;
                        }
                    },
                    HeadingAnchorMode::LinkInHeading => {
                        if let Some(id) = &id {
                            self.write(" <a href=\"#")?;
                            escape_href(&mut self.writer, &id)?;
                            self.write("\" class=\"heading-anchor\" aria-labelledby=\"")?;
                            escape_html(&mut self.writer, &id)?;
                            self.write("\">Section link</a>")?;
                        }
                    },
                    HeadingAnchorMode::LinkAfterHeading => (),
                }

                self.write("</")?;
                write!(&mut self.writer, "{}", level)?;
                self.write(">\n")?;

                match self.options.heading_anchor_links {
                    HeadingAnchorMode::None => (),
                    HeadingAnchorMode::HeadingIsLink => (),
                    HeadingAnchorMode::LinkInHeading => (),
                    HeadingAnchorMode::LinkAfterHeading => {
                        if let Some(id) = &id {
                            self.write("<a href=\"#")?;
                            escape_href(&mut self.writer, &id)?;
                            self.write("\" class=\"heading-anchor\" aria-labelledby=\"")?;
                            escape_html(&mut self.writer, &id)?;
                            self.write("\">Section link</a>")?;
                        }
                        self.write("</div>\n")?;
                    },
                }
            }
            TagEnd::Table => {
                self.write("</tbody></table>\n")?;
            }
            TagEnd::TableHead => {
                self.write("</tr></thead><tbody>\n")?;
                self.table_state = TableState::Body;
            }
            TagEnd::TableRow => {
                self.write("</tr>\n")?;
            }
            TagEnd::TableCell => {
                match self.table_state {
                    TableState::Head => {
                        self.write("</th>")?;
                    }
                    TableState::Body => {
                        self.write("</td>")?;
                    }
                }
                self.table_cell_index += 1;
            }
            TagEnd::BlockQuote => {
                self.write("</blockquote>\n")?;
            }
            TagEnd::CodeBlock => {
                self.write("</code></pre>\n")?;
            }
            TagEnd::List(true) => {
                self.write("</ol>\n")?;
            }
            TagEnd::List(false) => {
                self.write("</ul>\n")?;
            }
            TagEnd::Item => {
                self.write("</li>\n")?;
            }
            TagEnd::Emphasis => {
                self.write("</em>")?;
            }
            TagEnd::Strong => {
                self.write("</strong>")?;
            }
            TagEnd::Strikethrough => {
                self.write("</del>")?;
            }
            TagEnd::Link => {
                if let Some(link_data) = self.link_buffer.take() {
                    let content = self.writer.intercept.pop().unwrap();
                    // info!("Link {:?}", link_data);

                    self.write("<a href=\"")?;
                    let href = self.handle_url(&*link_data.dest_url)?;
                    escape_href(&mut self.writer, &href)?;
                    if !link_data.title.is_empty() {
                        self.write("\" title=\"")?;
                        escape_html(&mut self.writer, &link_data.title)?;
                    }

                    if content == "{embed}" {
                        self.write("\" embed>")?;
                    } else if content == "{embed-full}" {
                        self.write("\" embed=\"full\">")?;
                    } else {
                        self.write("\">")?;
                    }

                    self.write(&content)?;
                }

                self.write("</a>")?;
            }
            TagEnd::Image => (), // shouldn't happen, handled in start
            TagEnd::FootnoteDefinition => {
                self.write("</div>\n")?;
            }
            TagEnd::MetadataBlock(_) => {
                self.in_non_writing_block = false;
            }
        }
        Ok(())
    }

    // run raw text, consuming end tag
    fn raw_text(&mut self) -> Result<(), InterceptError<W::Error>> {
        let mut nest = 0;
        while let Some(event) = self.iter.next() {
            match event {
                Event::Start(_) => nest += 1,
                Event::End(_) => {
                    if nest == 0 {
                        break;
                    }
                    nest -= 1;
                }
                Event::Html(_) => {}
                Event::InlineHtml(text) | Event::Code(text) | Event::Text(text) => {
                    // Don't use escape_html_body_text here.
                    // The output of this function is used in the `alt` attribute.
                    escape_html(&mut self.writer, &text)?;
                    self.end_newline = text.ends_with('\n');
                }
                Event::InlineMath(text) => {
                    self.write("$")?;
                    escape_html(&mut self.writer, &text)?;
                    self.write("$")?;
                    self.end_newline = false;
                }
                Event::DisplayMath(text) => {
                    self.write("$$")?;
                    escape_html(&mut self.writer, &text)?;
                    self.write("$$")?;
                    self.end_newline = false;
                }
                Event::SoftBreak | Event::HardBreak | Event::Rule => {
                    self.write(" ")?;
                }
                Event::FootnoteReference(name) => {
                    let len = self.numbers.len() + 1;
                    let number = *self.numbers.entry(name).or_insert(len);
                    write!(&mut self.writer, "[{}]", number)?;
                }
                Event::TaskListMarker(true) => self.write("[x]")?,
                Event::TaskListMarker(false) => self.write("[ ]")?,
            }
        }
        Ok(())
    }
    
    fn handle_url<'b>(&self, dest_url: &'b str) -> Result<std::borrow::Cow<'b, str>, InterceptError<W::Error>> {
        if let Some(base) = &self.base_url {
            if dest_url.starts_with("/") || dest_url.starts_with("#") || dest_url.contains("://") {
                Ok(dest_url.into())
            } else {
                // TODO: handle relative base paths better
                // info!("start: {:?}, base: {:?}", dest_url, base);
                // info!("escaped: {:?}", href);
                // info!("Resolved: {:?}", base.join(&href));
                // let resolved = base.join(&href)
                //     .map(|u| u[url::Position::BeforePath..].to_owned().into())
                //     .unwrap_or_else(|_e| dest_url.into());
                let href = super::percent_encode(&format!("{}{}", base, dest_url));
                Ok(href.into())
            }
        } else {
            Ok(dest_url.into())
        }
    }
}

pub fn push_html<'a, I>(s: &mut String, iter: I, options: Options)
where
    I: Iterator<Item = Event<'a>>,
{
    HtmlWriter::new(iter, s, options).run().unwrap();
}

pub fn write_html<'a, I, W>(writer: W, iter: I, options: Options) -> Result<(), std::io::Error>
where
    I: Iterator<Item = Event<'a>>,
    W: Write,
{
    HtmlWriter::new(iter, IoWriter(writer), options).run().map_err(InterceptError::flatten)
}
