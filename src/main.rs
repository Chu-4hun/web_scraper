pub mod consts;
pub mod macros;

use core::panic;

use regex::Regex;
use scraper::{Html, Selector};
use tracing::debug;
use tracing_subscriber::{
    fmt::layer, layer::SubscriberExt, registry, util::SubscriberInitExt, EnvFilter,
};

use crate::consts::DEFAULT_LOG_FILTERS;

lazy_static::lazy_static! {
static ref PROJ_NAME_REGEX: Regex = Regex::new(
            r#"<span class="count-badge">(\d+)<\/span>"#,
        ).unwrap();}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut filter = EnvFilter::builder().with_env_var("LOG").from_env_lossy();

    for rule in DEFAULT_LOG_FILTERS {
        filter = filter.add_directive(rule.parse().expect("DEFAULT_LOG_FILTERS misconfiguration"));
    }
    registry().with(filter).with(layer()).init();

    let resp = reqwest::get("https://layboard.com/vakansii/chehiya")
        .await?
        .text()
        .await?;

    let document = Html::parse_document(&resp);
    let caps = &PROJ_NAME_REGEX.captures(&resp).unwrap()[0];

    let count_fragment = Html::parse_fragment(caps);
    
    let count_selector = Selector::parse("span.count-badge").unwrap();
    let job_cards_selector = Selector::parse("div.job-cards").unwrap();
    let vacancy_cards_selector = Selector::parse("a.vacancy-body").unwrap();


    let total_count = count_fragment
        .select(&count_selector)
        .next()
        .map(|elem| elem.text().collect::<String>())
        .unwrap_or(0.to_string())
        .parse::<usize>()
        .unwrap_or(0);

    debug!("{total_count:#?}",);
https://github.com/kxzk/scraping-with-rust
    let list = document.select(&job_cards_selector).next().unwrap();

    let urls = list.select(&vacancy_cards_selector).map(|elem| concat_str!("https://layboard.com" , elem.value().attr("href").unwrap()) ).collect::<Vec<String>>();
    debug!("{urls:#?}");
  
  for url in urls {
    let resp = reqwest::get(&url).await?.text().await?;
    let document = Html::parse_document(&resp);
    let job_title_selector = Selector::parse("div.jarticle__title").unwrap();
    let job_title = document.select(&job_title_selector).next().unwrap().text().collect::<String>();
    debug!("{job_title:#?}",);

  }



    Ok(())
}
