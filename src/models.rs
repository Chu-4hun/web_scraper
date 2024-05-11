use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Vacancy {
    pub site: usize,
    #[serde(rename = "URL")]
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub date: Option<String>,
    pub salary: Option<String>,
    pub visa: Option<bool>,
    pub experience: Option<bool>,
    pub language: Option<String>,
}
