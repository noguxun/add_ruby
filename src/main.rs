use anyhow::Result;
use chrono::Utc;
use fastly::http::{header, HeaderValue, Method, StatusCode};
use fastly::{dictionary::Dictionary, Body, Error, Request, RequestExt, Response, ResponseExt};
use http::header::{ACCEPT_ENCODING, CONTENT_TYPE, LOCATION};
use kanji::{is_hiragana, is_kanji};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

const API_BACKEND: &str = "labs.goo.ne.jp";
const BACKEND_NAME: &str = "www.fastly.jp";
const LOG: &str = "PaperTrail";

#[derive(Serialize, Deserialize)]
struct HiraganaResp {
    converted: String,
    output_type: String,
    request_id: String,
}

struct HtmlPart {
    content: String,
    need_ruby: bool,
}

#[fastly::main]
fn main(mut req: Request<Body>) -> Result<impl ResponseExt, Error> {
    // set log endpoint
    fastly::log::set_panic_endpoint(LOG).unwrap();
    log_fastly::init_simple(LOG, log::LevelFilter::Info);

    // Make any desired changes to the client request.
    req.headers_mut()
        .insert("Host", HeaderValue::from_static(BACKEND_NAME));
    req.headers_mut().remove(ACCEPT_ENCODING);

    // We can filter requests that have unexpected methods.
    const VALID_METHODS: [Method; 3] = [Method::HEAD, Method::GET, Method::POST];
    if !(VALID_METHODS.contains(req.method())) {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::from("This method is not allowed"))?);
    }

    // Request handling logic could go here...
    req.set_pass();
    log::info!("time: {},url: {}", Utc::now(), req.uri());
    let mut resp = req.send(BACKEND_NAME)?;
    if resp.status() == StatusCode::MOVED_PERMANENTLY {
        let re = Regex::new(r"https?://www\.fastly\.jp/.*$").unwrap();
        let location = resp.headers().get(LOCATION).unwrap().to_str().unwrap();
        if re.is_match(location) {
            let req = Request::get(location).body(()).unwrap();
            resp = req.send(BACKEND_NAME)?;
        }
    }
    if resp.status() == StatusCode::OK && resp.headers().get(CONTENT_TYPE).unwrap() == "text/html" {
        let body_string = resp.into_body().into_string();
        log::info!(
            "time: {}, Get response body from the content site",
            Utc::now()
        );
        let (html_parts, jp_content) = analyze_jp(&body_string);
        let coverted = generate_html_with_ruby(&html_parts, &jp_content)?;
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(coverted))?);
    }
    Ok(resp)
}

fn analyze_jp(body_string: &str) -> (Vec<HtmlPart>, String) {
    let chars_num = body_string.chars().count();
    let html_chars = body_string.chars().collect::<Vec<char>>();
    let mut i = 0;
    let mut html_parts = Vec::new();
    let mut content = "".to_string();
    let mut jp_content = "".to_string();
    while i < chars_num {
        let mut ch = html_chars[i];
        if ch != '>' {
            content.push(ch);
            i += 1;
            continue;
        }
        if ch == '>' {
            loop {
                ch = html_chars[i];
                let next_char;
                if i + 1 < chars_num {
                    next_char = html_chars[i + 1]
                } else {
                    content.push(ch);
                    let html_part = HtmlPart {
                        content: content.clone(),
                        need_ruby: false,
                    };
                    html_parts.push(html_part);
                    break;
                }
                if next_char == '<' {
                    if !is_kanji(&ch) && !is_hiragana(&ch) {
                        content.push(ch);
                        i += 1;
                        break;
                    } else {
                        content.push(ch);
                        i += 1;
                        jp_content = format!("{}{},", jp_content, content);

                        let html_part = HtmlPart {
                            content: content,
                            need_ruby: true,
                        };

                        html_parts.push(html_part);
                        content = "".to_string();
                        break;
                    }
                }
                if !is_kanji(&next_char) && !is_hiragana(&next_char) {
                    if !is_kanji(&ch) && !is_hiragana(&ch) {
                        content.push(ch);
                        i += 1;
                    } else {
                        content.push(ch);
                        i += 1;
                        jp_content = format!("{}{},", jp_content, content);

                        let html_part = HtmlPart {
                            content: content,
                            need_ruby: true,
                        };
                        html_parts.push(html_part);

                        content = "".to_string();
                    }
                } else {
                    if !is_kanji(&ch) && !is_hiragana(&ch) {
                        content.push(ch);
                        i += 1;
                        let html_part = HtmlPart {
                            content: content,
                            need_ruby: false,
                        };
                        html_parts.push(html_part);
                        content = "".to_string();
                    } else {
                        content.push(ch);
                        i += 1;
                    }
                }
            }
        }
    }
    return (html_parts, jp_content);
}

fn generate_html_with_ruby(parts: &Vec<HtmlPart>, jp_content: &str) -> Result<String> {
    let mut html_page = String::new();
    let hiragana = get_hiragana(jp_content)?;
    let ruby: Vec<&str> = hiragana.as_str().split(',').collect();
    let mut i = 0;
    for part in parts {
        log::info!("content: {}", part.content);
        if part.need_ruby {
            log::info!("<ruby><rb>{}</rb><rt>{}</rt></ruby>", part.content, ruby[i]);
            write!(
                &mut html_page,
                "<ruby><rb>{}</rb><rt>{}</rt></ruby>",
                part.content, ruby[i]
            )?;
            i += 1;
        } else {
            write!(&mut html_page, "{}", part.content)?;
        }
    }

    Ok(html_page)
}

fn get_hiragana(j: &str) -> Result<String> {
    let api_config = Dictionary::open("api_config");
    let app_id = api_config.get("api_id").unwrap();
    let output_type = api_config.get("output_type").unwrap();
    let req_body = format!(
        r#"{{"app_id": "{}","sentence": "{}","output_type": "{}"}}"#,
        app_id, j, output_type
    );

    log::info!("{}", &req_body);

    let req = Request::builder()
        .method(Method::POST)
        .header(header::CONTENT_TYPE, "application/json")
        .uri("https://labs.goo.ne.jp/api/hiragana")
        .body(Body::from(req_body))?;

    let resp = req.send(API_BACKEND)?;

    let body_str = resp.into_body().into_string();

    log::info!("{}", &body_str);

    let hiragana_resp: HiraganaResp = serde_json::from_str(&body_str)?;

    Ok(hiragana_resp.converted)
}
