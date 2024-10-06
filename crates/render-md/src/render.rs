
use std::path::Path;
use std::future::Future;

// use runtime::utils::enclose;
use crate::coawait::{coro_await, UnsafeSendWrapper};



#[derive(serde::Deserialize, Default)]
pub struct Metadata {
    pub title: Option<String>,
    #[serde(flatten)]
    pub _rest: std::collections::HashMap<String, serde_yaml::Value>,
}

pub async fn rewrite_html<F, Fu>(
    html: String,
    base_url: Option<&str>,
    handle_embed: F,
) -> Result<String, anyhow::Error>
    where F: Fn(url::Url, bool) -> Fu,
        Fu: Future<Output = Result<Option<String>, anyhow::Error>>,
{
    use lol_html::{rewrite_str, element, RewriteStrSettings};

    let base_url = base_url.and_then(|s| url::Url::parse(s).ok());

    #[tracing::instrument(skip(handle_embed))]
    async fn render_embed<F, Fu>(
        url: &str,
        base_url: &Option<url::Url>,
        preview: bool,
        handle_embed: F,
    ) -> Result<Option<String>, anyhow::Error>
        where F: Fn(url::Url, bool) -> Fu,
            Fu: Future<Output = Result<Option<String>, anyhow::Error>>,
    {
        if let Ok(Some(resolved_url)) = url::Url::parse(&url).map(Some)
            .or_else(|_| base_url.as_ref().map(|b| b.join(&url)).transpose())
        {
            handle_embed(resolved_url, preview).await
        } else {
            // Bad URL
            Ok(None)
        }
    }

    let future = async move {
        coro_await(move |awaiter| {
            // let buffer = std::rc::Rc::new(std::cell::RefCell::new(String::new()));

            // struct Abc<'a>(std::cell::RefCell<Vec<Box<dyn Fn() + 'a>>>);
            // impl<'a> Abc<'a> {
            //     fn add(&self, f: impl Fn() + 'a) {
            //         self.0.borrow_mut().push(Box::new(f));
            //     }
            // }

            // let closures = Abc(std::cell::RefCell::new(Vec::new()));

            // let a = 0;
            // struct B<'a>(&'a u32);
            // let b = B(&a);

            // let k: Box<dyn Fn(&mut lol_html::html_content::EndTag<'_>) -> lol_html::HandlerResult + '_> = Box::new(enclose!([ref a, ref b] move |end: &mut lol_html::html_content::EndTag<'_>| {
            //     // awaiter;
            //     a;
            //     // b;
            //     Ok(())
            // }));

            let element_content_handlers = vec![
                // element!("link-embed", |el: &mut lol_html::html_content::Element<'_, '_, '_>| {
                //     // info!("link embed running");
                //     el.remove();
                //     buffer.borrow_mut().clear();
                //     closures.add(|| {
                //         let v = awaiter.block_on(async {});
                //     });

                //     if let Some(handlers) = el.end_tag_handlers() {
                //         let h: Box<dyn FnOnce(&mut lol_html::html_content::EndTag<'_>) -> lol_html::HandlerResult + '_> = Box::new(enclose!([ref a, ref b] move |end: &mut lol_html::html_content::EndTag<'_>| {
                //             // awaiter;
                //             a;
                //             b;
                //             Ok(())
                //         }));
                //         handlers.push(Box::new(|a| k(a)));
                //         // handlers.push(Box::new(enclose!([clone buffer, ref base_url] move |end| {
                //         //     // let url = buffer.borrow().clone();
                //         //     let x = awaiter;
                //         //     // yielder.block_on
                //         //     // let rendered = {
                //         //     //     let yielder = token.borrow();
                //         //     //     let yielder = yielder.as_ref().unwrap();
                //         //     //     yielder.block_on(async {
                //         //     //         render_embed(&url, app, &base_url).await
                //         //     //     })
                //         //     // };
                //         //     // if let Ok(Some(rendered)) = rendered {
                //         //     //     end.replace(&rendered, lol_html::html_content::ContentType::Html);
                //         //     // } else {
                //         //     //     let rendered = format!("<span>Broken embed: {}</span>", html_escape_not_unquoted(&*buffer.borrow()));
                //         //     //     end.replace(&rendered, lol_html::html_content::ContentType::Html);
                //         //     // }
                //         //     Ok(())
                //         // })));
                //     }
                //     Ok(())
                // }),
                // text!("link-embed", |text| {
                //     // info!("link embed text running {:?}", text);
                //     buffer.borrow_mut().push_str(text.as_str());
                //     Ok(())
                // }),

                element!("a[embed]", |el| {
                    if let Some(url) = el.get_attribute("href") {
                        let preview = matches!(el.get_attribute("embed").as_deref(), Some("full"));
                        let rendered = awaiter.block_on(async {
                            render_embed(&url, &base_url, preview, &handle_embed).await
                        });
                        if let Ok(Some(rendered)) = rendered {
                            el.replace(&rendered, lol_html::html_content::ContentType::Html);
                        }
                    }
                    Ok(())
                }),

                element!("a[href^='##']", |el| {
                    if let Some(url) = el.get_attribute("href") {
                        // TODO: this is a html injection vuln (user can put in ##<script>...</script>##about:blank as url)
                        // Needs a randomized (and filtered) hash for prefix
                        if let Some((label, new_url)) = &url[2..].split_once("##") {
                            el.set_attribute("href", new_url).ok();
                            el.set_inner_content(label, lol_html::html_content::ContentType::Html);
                        }
                    }
                    Ok(())
                }),
            ];

            let res = rewrite_str(
                &html,
                RewriteStrSettings {
                    element_content_handlers,
                    ..RewriteStrSettings::default()
                }
            );

            // drop(closures);

            // let b = res.unwrap();
            // Ok(b)
            res
        }).await
            .map_err(Into::into)
    };

    // Safety:
    // 
    // lol_html internally uses `Rc`s and `RefCell`s, so it is not safe to use across
    // multiple threads.  However, the `Rc`s can never escape this future/task, it is
    // safe to use within the current task.
    // 
    // Neither corosensei/lol_html use thread local variables, so they do not depend on
    // the OS thread at all -- thus this *task* can be sent between OS threads, even
    // though its contents cannot be sent between *tasks*.
    // 
    // Somewhat relevant: https://matklad.github.io/2023/12/10/nsfw.html
    unsafe { UnsafeSendWrapper::new(future) }.await
}

#[tracing::instrument(skip(handle_embed))]
pub async fn render_page_markdown<F, Fu>(
    path: &Path,
    base_url: Option<&str>,
    handle_embed: F,
) -> Result<Option<(String, Metadata)>, anyhow::Error>
    where F: Fn(url::Url, bool) -> Fu,
        Fu: Future<Output = Result<Option<String>, anyhow::Error>>,
{
    let md = match fs_err::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        },
        Err(e) => return Err(e.into()),
    };
    // TODO: provide base URL for relative links?
    let (mut html, meta) = crate::process_markdown(&md, base_url);
    let meta = meta.first().map(|s| serde_yaml::from_str::<Metadata>(&s)).transpose();
    let meta = match meta {
        Ok(Some(m)) => m,
        Ok(None) => Metadata::default(),
        Err(e) => {
            // TODO: pass this warning back to the user
            warn!("Error parsing metadata: {}", e);
            Metadata::default()
        }
    };

    html = rewrite_html(html, base_url, handle_embed).await?;

    // let sanitized = runtime::template::sanitize_html_trusted(&html);
    let sanitized = html;
    Ok(Some((sanitized, meta)))
}
