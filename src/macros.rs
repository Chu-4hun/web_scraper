#[macro_export]
macro_rules! concat_str {
    ($($item:expr),* $(,)?) => {
        [$($item,)*].concat()
    };
}
