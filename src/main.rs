pub mod consts;
pub mod macros;
pub mod models;
pub mod opts;

use std::sync::Arc;

use anyhow::bail;
use clap::Parser;
use futures::{stream::TryStreamExt, StreamExt};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use tracing::{debug, info};
use tracing_subscriber::{
    fmt::layer, layer::SubscriberExt, registry, util::SubscriberInitExt, EnvFilter,
};

use crate::{consts::DEFAULT_LOG_FILTERS, models::Vacancy, opts::Opts};

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

    }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Arc::new(Opts::parse());

    let mut filter = EnvFilter::builder().with_env_var("LOG").from_env_lossy();

    for rule in DEFAULT_LOG_FILTERS {
        filter = filter.add_directive(rule.parse().expect("DEFAULT_LOG_FILTERS misconfiguration"));
    }
    registry().with(filter).with(layer()).init();

    let client = Arc::new(reqwest::ClientBuilder::new().build()?);
    let (total_count, init_urls) =
        parse_page(&client, "https://layboard.com/vakansii/chehiya", true).await?;

    let init_len = init_urls.len() as f64;
    let res = process_urls(client.clone(), opts.clone(), init_urls).await;

    debug!("initial run total: {} page size {}", total_count, init_len);
    if res.is_err() {
        info!("parsing stopped {res:?}");
        return Ok(());
    }

    for page in 2..=((total_count as f64 / init_len).ceil() as usize) {
        debug!("parsing page: {page}");
        let (_, urls) = parse_page(
            &client,
            &format!("https://layboard.com/vakansii/chehiya?page={}", page),
            false,
        )
        .await?;

        debug!("{urls:?}");

        let res = process_urls(client.clone(), opts.clone(), urls).await;

        if res.is_err() {
            break;
        }
    }

    Ok(())
}

async fn process_urls(
    client: Arc<Client>,
    opts: Arc<Opts>,
    urls: Vec<String>,
) -> anyhow::Result<()> {
    futures::stream::iter(urls)
        .map(Ok)
        .try_for_each_concurrent(1, |url| {
            let client_ref = Arc::as_ref(&client);
            let opts = Arc::as_ref(&opts);
            async move {
                parse_vacancy(client_ref, opts, &concat_str!("https://layboard.com", &url)).await
            }
        })
        .await
}

async fn parse_page(
    client: &reqwest::Client,
    url: &str,
    get_total: bool,
) -> anyhow::Result<(usize, Vec<String>)> {
    let resp = client.get(url).send().await?.text().await?;

    let document = Html::parse_document(&resp);

    let total_count =
     if get_total {
        let caps = &PROJ_NAME_REGEX.captures(&resp).unwrap()[0];
        let count_fragment = Html::parse_fragment(caps);

        count_fragment
            .select(&COUNT_SELECTOR)
            .next()
            .map(|elem| elem.text().collect::<String>())
            .unwrap_or(0.to_string())
            .parse::<usize>()
            .unwrap_or(0)
    } else {
        0
    };

    let urls = document
        .select(&JOB_SELECTOR)
        .map(|i| i.value().attr("href").unwrap().to_string())
        .collect::<Vec<String>>();

    Ok((total_count, urls))
}

async fn parse_vacancy(client: &reqwest::Client, opts: &Opts, url: &str) -> anyhow::Result<()> {
    debug!("parsing url: {url}");
    let resp = client.get(url).send().await?.text().await?;

    let document = Html::parse_document(&resp);

    let mut description = "".to_string();
    // debug!("{:#?}", document
    //     .select(&JOB_TITLE_SELECTOR).collect::<Vec<_>>());
    let title = document
        .select(&JOB_TITLE_SELECTOR)
        .next()
        .unwrap()
        .text()
        .collect::<String>();

    document
        .select(&JOB_DESCRIPTION_SELECTOR)
        .for_each(|i| description.push_str(i.text().collect::<String>().trim()));

    let salary = document
        .select(&JOB_SALARY_SELECTOR)
        .next()
        .unwrap()
        .text()
        .collect::<String>();

    let is_visa = document.select(&JOB_VISA_SELECTOR).next().is_some();
    let is_exp = document.select(&JOB_EXP_SELECTOR).next().is_some();
    let is_no_exp = document.select(&JOB_NO_EXP_SELECTOR).next().is_some();
    let is_hot = document.select(&JOB_IS_HOT_SELECTOR).next().is_some();

    let is_exp_needed = {
        if is_exp {
            Some(true)
        } else if is_no_exp {
            Some(false)
        } else {
            None
        }
    };

    let vac = Vacancy {
        site: opts.site_id,
        url: url.to_string(),
        title,
        description: Some(description),
        date: None,
        salary: Some(salary),
        visa: Some(is_visa),
        experience: is_exp_needed,
        language: None,
    };
    let resp = client
        .post(&concat_str!(opts.url.clone(), "/vacancies".to_string()))
        .json(&vac)
        .send()
        .await?;

    if is_hot {
        info!("hot vacancy found: {url}");
    } else if resp.status().is_client_error() {
        info!("parsing stopped \n{:#?}", resp.text().await?);
        bail!("parsing stopped");
    }
    Ok(())
}
