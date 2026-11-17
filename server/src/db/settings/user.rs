//! Persisted user settings; key names remain compatible with existing accounts.
pub(in crate::db) enum UserSetting<'a> {
    Methodology,
    WatchRules,
    FxBook(&'a str),
    FxQuote {
        from: &'a str,
        to: &'a str,
        on_date: &'a str,
    },
}
impl UserSetting<'_> {
    pub(in crate::db) const FX_BOOK_PREFIX: &'static str = "fx.book.";
    pub(in crate::db) const FX_QUOTE_PREFIX: &'static str = "fx.quote.";
    pub(in crate::db) fn key(&self) -> String {
        match self {
            Self::Methodology => "investment.methodology".into(),
            Self::WatchRules => "watch.rules".into(),
            Self::FxBook(base) => format!("{}{base}", Self::FX_BOOK_PREFIX),
            Self::FxQuote { from, to, on_date } => {
                format!("{}{from}.{to}.{on_date}", Self::FX_QUOTE_PREFIX)
            }
        }
    }
}
