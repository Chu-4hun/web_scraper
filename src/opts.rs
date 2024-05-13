use clap::Parser;
use tracing::level_filters::LevelFilter;

#[derive(Parser, Debug,Clone)]
#[command(version, about, long_about = None)]
pub struct Opts {
    /// Log level of application
    #[arg(global = true, short, long, env, default_value_t = LevelFilter::INFO)]
    log: LevelFilter,

    #[arg(global = true, short, long, env, default_value_t = 1)]
   pub site_id: usize,
    #[arg(global = true, short, long, env, default_value_t = 1)]
   pub threads: usize,

    #[arg(global = true, short, long, env, default_value_t = String::from("http://localhost:8081"))]
   pub url: String,
}