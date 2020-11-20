use anyhow::{anyhow, Result};
use fastly::http::{header, HeaderValue, Method, StatusCode};
use fastly::request::CacheOverride;
use fastly::{Body, Error, Request, RequestExt, Response, ResponseExt};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

const BACKEND_NAME: &str = "labs.goo.ne.jp";

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
fn main(req: Request<Body>) -> Result<impl ResponseExt, Error> {
    log_fastly::init_simple("my_log", log::LevelFilter::Info);
    fastly::log::set_panic_endpoint("my_log").unwrap();

    // We can filter requests that have unexpected methods.
    const VALID_METHODS: [Method; 3] = [Method::HEAD, Method::GET, Method::POST];
    if !(VALID_METHODS.contains(req.method())) {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::from("This method is not allowed"))?);
    }

    // Pattern match on the request method and path.
    match (req.method(), req.uri().path()) {
        // If request is a `GET` to the `/` path, send a default response.
        (&Method::GET, "/welcome") => Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("Welcome to Fastly Compute@Edge!"))?),

        // If request is a `GET` to the `/backend` path, send to a named backend.
        (&Method::GET, "/test1") => {
            let html_parts = generate_sample_html_parts();
            let coverted = generate_html_with_ruby(&html_parts)?;
            
            Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(coverted))?)
        }

        // Catch all other requests and return a 404.
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("The page you requested could not be found"))?),
    }
}

fn generate_sample_html_parts() -> Vec<HtmlPart> {
    let mut parts = Vec::<HtmlPart>::new();

    parts.push(HtmlPart {
        content: String::from("<html>"),
        need_ruby: false,
    });

    parts.push(HtmlPart {
        content: String::from("日本語が難しいですよ"),
        need_ruby: true,
    });

    parts.push(HtmlPart {
        content: String::from("</html>"),
        need_ruby: false,
    });

    parts
}

fn generate_html_with_ruby(parts: &Vec<HtmlPart>) -> Result<String>{
    let mut html_page = String::new();
    for part in parts {
        if part.need_ruby {
            let hiragana = get_hiragana(&part.content)?;
            write!(&mut html_page, "<ruby><rb>{}</rb><rt>{}</rt></ruby>", part.content, hiragana);
        }
        else {
            write!(&mut html_page, "{}", part.content);
        }
    }

    Ok(html_page)
}

fn get_hiragana(j: &str) -> Result<String> {
    let app_id = "57612e6db386dded03ab099ac9afa1276ea7f20f78528b8a5a0717e0e99b69e2";
    let req_body = format!(
        r#"{{
      "app_id": "{}",
      "request_id": "test1",
      "sentence": "{}",
      "output_type": "hiragana"
    }}"#,
        app_id, j
    );

    log::info!("{}", &req_body);

    let req = Request::builder()
        .method(Method::POST)
        .header(header::CONTENT_TYPE, "application/json")
        .uri("https://labs.goo.ne.jp/api/hiragana")
        .body(Body::from(req_body))?;

    let resp = req.send(BACKEND_NAME)?;

    let (_parts, body) = resp.into_parts();
    let body_str = body.into_string();

    log::info!("{}", &body_str);

    let hiragana_resp: HiraganaResp = serde_json::from_str(&body_str)?;

    Ok(hiragana_resp.converted)
}
