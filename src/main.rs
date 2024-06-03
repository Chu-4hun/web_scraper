pub mod consts;
pub mod macros;
pub mod models;
pub mod opts;

use std::{collections::HashSet, str::FromStr, sync::Arc, time::SystemTime, vec};

use anyhow::bail;
use clap::Parser;
use futures::stream::{self, StreamExt};
use models::{Author, Vacancy};
use regex::Regex;
use reqwest::{header, multipart, Body, Client};
use scraper::{Html, Selector};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};
use tracing::{debug, info};
use tracing_subscriber::{
    fmt::{format, layer},
    layer::SubscriberExt,
    registry,
    util::SubscriberInitExt,
    EnvFilter,
};

use crate::{consts::DEFAULT_LOG_FILTERS, opts::Opts};

lazy_static::lazy_static! {
static ref PROJ_NAME_REGEX: Regex = Regex::new(
            r#"<span class="count-badge">(\d+)<\/span>"#,
        ).unwrap();

    static ref JOB_SELECTOR : Selector = Selector::parse("a.vacancy-body").unwrap();
    static ref COUNT_SELECTOR : Selector= Selector::parse("span.count-badge").unwrap();
    static ref JOB_TITLE_SELECTOR : Selector = Selector::parse("div.jarticle__title").unwrap();
    static ref JOB_DESCRIPTION_SELECTOR: Selector = Selector::parse("div.jarticle__descrip").unwrap();
    static ref JOB_SALARY_SELECTOR : Selector= Selector::parse("div.jarticle__stat-value").unwrap();
    static ref JOB_VISA_SELECTOR: Selector =
        Selector::parse(r#"a[href="/vakansii/chehiya/filter-rabochaya-viza"]"#).unwrap();
    static ref JOB_EXP_SELECTOR: Selector =
        Selector::parse(r#"a[href="/vakansii/chehiya/filter-trebuetsya-oput"]"#).unwrap();
    static ref JOB_NO_EXP_SELECTOR: Selector =
        Selector::parse(r#"a[href="/vakansii/chehiya/filter-bez-oputa"]"#).unwrap();
    static ref JOB_IS_HOT_SELECTOR: Selector = Selector::parse("div.hot-btn").unwrap();
    static ref JOB_IS_HOT_BADGE_SELECTOR: Selector = Selector::parse("div.hot-badge").unwrap();

}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Arc::new(Opts::parse());

    let mut filter = EnvFilter::builder().with_env_var("LOG").from_env_lossy();

    for rule in DEFAULT_LOG_FILTERS {
        filter = filter.add_directive(rule.parse().expect("DEFAULT_LOG_FILTERS misconfiguration"));
    }
    registry().with(filter).with(layer()).init();

    let mut headers = header::HeaderMap::new();
    let mut auth_value = header::HeaderValue::from_str(&opts.key)?;
    auth_value.set_sensitive(true);
    headers.insert("Api-Key", auth_value);

    let client = Arc::new(
        reqwest::ClientBuilder::new()
            .default_headers(headers)
            .build()?,
    );

    let (resp, document) = get_page_html(&client, "https://layboard.com/vakansii/chehiya").await?;
    let total_count = get_total_count(resp);
    let ignore_urls: HashSet<String> = HashSet::from_iter(
        get_filter_urls(&document)
            .into_iter()
            .map(|f| format!("https://layboard.com{}", f)),
    );
    debug!("ignore_urls = {ignore_urls:?}");
    let init_urls: Vec<String> = get_urls(document);

    let init_len = init_urls.len() as f64;
    let mut vacancies = vec![];
    for url in init_urls {
        vacancies.push(
            parse_vacancy(
                Arc::as_ref(&client),
                Arc::as_ref(&opts),
                &concat_str!("https://layboard.com", &url),
                &ignore_urls,
            )
            .await?,
        );
    }
  
    for page in 2..=((total_count as f64 / init_len).ceil() as usize) {
        info!("parsing page: {page}");
        let urls = parse_page(
            &client,
            &format!("https://layboard.com/vakansii/chehiya?page={}", page),
        )
        .await?;
        for url in urls {
            vacancies.push(
                parse_vacancy(
                    Arc::as_ref(&client),
                    Arc::as_ref(&opts),
                    &concat_str!("https://layboard.com", &url),
                    &ignore_urls,
                )
                .await?,
            );
        }
        debug!("vac {:#?}", vacancies);
    }
    send_file(&vacancies, &client, &opts).await?;

    Ok(())
}

async fn send_file(vacancies: &Vec<Vacancy>, client: &Client, opts: &Opts) -> anyhow::Result<()> {
    let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    let filename = format!("layboard.com-{}.jsonl", time.as_secs());
    serde_jsonlines::write_json_lines(filename.clone(), vacancies)?;

    let file = File::open(filename.as_str()).await?;
    // read file body stream
    let stream = FramedRead::new(file, BytesCodec::new());
    let file_body = Body::wrap_stream(stream);

    //make form part of file
    let some_file = multipart::Part::stream(file_body)
        .file_name(filename)
        .mime_str("text/plain")?;

    let form = multipart::Form::new().part("file", some_file);
    // https://base.eriar.com/api/ads/import
    let url = opts.url.clone();
    let response = client
        .post(concat_str!(url, "/api/ads/import".to_string()))
        .multipart(form)
        .send()
        .await?;
    debug!("export res: \n{} ", response.text().await?);
    Ok(())
}

async fn parse_page(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<String>> {
    let document = Html::parse_document(&client.get(url).send().await?.text().await?);
    let urls = get_urls(document);
    Ok(urls)
}

fn get_urls(document: Html) -> Vec<String> {
    let urls = document
        .select(&JOB_SELECTOR)
        .map(|i| i.value().attr("href").unwrap().to_string())
        .collect::<Vec<String>>();
    urls
}

fn get_filter_urls(document: &Html) -> Vec<String> {
    document
        .select(&JOB_SELECTOR)
        .filter_map(|elem| {
            if elem.select(&JOB_IS_HOT_BADGE_SELECTOR).next().is_some() {
                Some(elem.value().attr("href").unwrap().to_string())
            } else {
                None
            }
        })
        .collect::<Vec<String>>()
}

fn get_total_count(input_text: String) -> usize {
    let caps = &PROJ_NAME_REGEX.captures(&input_text).unwrap()[0];
    let count_fragment = Html::parse_fragment(caps);

    count_fragment
        .select(&COUNT_SELECTOR)
        .next()
        .map(|elem| elem.text().collect::<String>())
        .unwrap_or(0.to_string())
        .parse::<usize>()
        .unwrap_or(0)
}

async fn get_page_html(client: &Client, url: &str) -> anyhow::Result<(String, Html)> {
    let resp = client.get(url).send().await?.text().await?;
    let document = Html::parse_document(&resp);
    Ok((resp, document))
}

async fn parse_vacancy(
    client: &reqwest::Client,
    opts: &Opts,
    url: &str,
    ignore_urls: &HashSet<String>,
) -> anyhow::Result<Vacancy> {
    debug!("parsing url: {url}");
    let resp = client.get(url).send().await?.text().await?;

    let document = Html::parse_document(&resp);

    let mut description = " ".to_string();
    let mut title = " ".to_string();
    if let Some(_title) = document.select(&JOB_TITLE_SELECTOR).next() {
        title = _title.text().collect::<String>();
    }

    document
        .select(&JOB_DESCRIPTION_SELECTOR)
        .for_each(|i| description.push_str(i.text().collect::<String>().trim()));

    // let salary = document
    //     .select(&JOB_SALARY_SELECTOR)
    //     .next()
    //     .unwrap()
    //     .text()
    //     .collect::<String>();

    let is_hot = document.select(&JOB_IS_HOT_SELECTOR).next().is_some();

    let vac = Vacancy {
        type_id: 1,
        view_url: url.to_string(),
        title,
        description,
        name: String::from_str("layboard.com")?,
        author:Author{name: "layboard.com".to_string()}
    };
    // let resp = client
    //     .post(&concat_str!(opts.url.clone(), "/vacancies".to_string()))
    //     .json(&vac)
    //     .send()
    //     .await?;
    if is_hot || ignore_urls.contains(url) {
        info!("hot vacancy found: {url}");
    }
    // else {
    //     bail!("parsing stopped");
    // }
    Ok(vac)
}
