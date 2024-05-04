pub mod consts;
pub mod macros;
pub mod models;
pub mod opts;

use clap::Parser;
use regex::Regex;
use scraper::{Html, Selector};
use tracing::debug;
use tracing_subscriber::{
    fmt::layer, layer::SubscriberExt, registry, util::SubscriberInitExt, EnvFilter,
};

use crate::{consts::DEFAULT_LOG_FILTERS, models::Vacancy, opts::Opts};

lazy_static::lazy_static! {
static ref PROJ_NAME_REGEX: Regex = Regex::new(
            r#"<span class="count-badge">(\d+)<\/span>"#,
        ).unwrap();}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();

    let mut filter = EnvFilter::builder().with_env_var("LOG").from_env_lossy();

    for rule in DEFAULT_LOG_FILTERS {
        filter = filter.add_directive(rule.parse().expect("DEFAULT_LOG_FILTERS misconfiguration"));
    }
    registry().with(filter).with(layer()).init();

    let client = reqwest::ClientBuilder::new().build()?;

    let resp = client
        .get("https://layboard.com/vakansii/chehiya")
        .send()
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
    let list = document.select(&job_cards_selector).next().unwrap();

    let urls = list
        .select(&vacancy_cards_selector)
        .map(|elem| concat_str!("https://layboard.com", elem.value().attr("href").unwrap()))
        .collect::<Vec<String>>();
    debug!("{urls:#?}");

    for url in urls {
        let resp = client.get(&url).send().await?.text().await?;
        let document = Html::parse_document(&resp);
        let job_title_selector = Selector::parse("div.jarticle__title").unwrap();
        let job_description_selector = Selector::parse("div.jarticle__descrip").unwrap();
        let job_salary_selector = Selector::parse("div.jarticle__stat-value").unwrap();
        let job_visa_selector =
            Selector::parse(r#"a[href="/vakansii/chehiya/filter-rabochaya-viza"]"#).unwrap();
        let job_exp_selector =
            Selector::parse(r#"a[href="/vakansii/chehiya/filter-trebuetsya-oput"]"#).unwrap();
        let job_no_exp_selector =
            Selector::parse(r#"a[href="/vakansii/chehiya/filter-bez-oputa"]"#).unwrap();

        let mut description = "".to_string();
        let title = document
            .select(&job_title_selector)
            .next()
            .unwrap()
            .text()
            .collect::<String>();
        document
            .select(&job_description_selector)
            .for_each(|i| description.push_str(i.text().collect::<String>().trim()));

        let salary = document
            .select(&job_salary_selector)
            .next()
            .unwrap()
            .text()
            .collect::<String>();

        let is_visa = document.select(&job_visa_selector).next().is_some();
        let is_exp = document.select(&job_exp_selector).next().is_some();
        let is_no_exp = document.select(&job_no_exp_selector).next().is_some();

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
            url,
            title,
            description: Some(description),
            date: None,
            salary: Some(salary),
            visa: Some(is_visa),
            experience: is_exp_needed,
            language: None,
        };
        client
            .post(&concat_str!(opts.url.clone(), "/vacancies".to_string()))
            .json(&vac)
            .send()
            .await?;
        debug!("{is_visa} {is_exp_needed:?}",);
    }

    Ok(())
}
