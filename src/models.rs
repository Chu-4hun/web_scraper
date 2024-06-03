use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Vacancy {
    pub type_id: i64,
    pub title: String,
    pub description: String,
    pub view_url: String,
    pub name: String,
    pub author: Author,
}


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Author {
    pub name: String,
}
