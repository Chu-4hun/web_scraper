pub(crate) const DEFAULT_LOG_FILTERS: &[&str] = &[
    #[cfg(not(debug_assertions))]
    "mio=info",
    "hyper_util=info",
    "reqwest=info",
    "rustls=info",
    "hickory_resolver=info",
    "hickory_proto=info",
    "html5ever=info",
    "selectors=info",
];
