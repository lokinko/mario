//! Public market identities shared by source parsing and persisted-evidence validation.
#[derive(Clone, Debug)]
pub(crate) struct Security {
    pub market: u16,
    pub code: String,
    pub currency: &'static str,
    pub exchange: &'static str,
    pub mic: &'static str,
}

pub(crate) fn security(symbol: &str, exchange: &str) -> Option<Security> {
    let (market, code, currency, expected, mic) = if let Some(code) = symbol.strip_suffix(".SS") {
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        (1, code.to_owned(), "CNY", "SHH", "XSHG")
    } else if let Some(code) = symbol.strip_suffix(".SZ") {
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        (0, code.to_owned(), "CNY", "SHZ", "XSHE")
    } else if let Some(code) = symbol.strip_suffix(".HK") {
        if !(1..=5).contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let number = code.parse::<u32>().ok()?;
        // RMB counters and non-equity HK instruments require other currency metadata.
        if number == 0 || number >= 80000 {
            return None;
        }
        (116, format!("{number:05}"), "HKD", "HKG", "XHKG")
    } else if !symbol.is_empty()
        && symbol
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        match exchange {
            "NASDAQ" => (105, symbol.to_owned(), "USD", "NASDAQ", "XNAS"),
            "NYSE" => (106, symbol.to_owned(), "USD", "NYSE", "XNYS"),
            "AMEX" => (107, symbol.to_owned(), "USD", "AMEX", "XASE"),
            _ => return None,
        }
    } else {
        return None;
    };
    (exchange == expected).then_some(Security {
        market,
        code,
        currency,
        exchange: expected,
        mic,
    })
}

impl Security {
    pub fn secid(&self) -> String {
        format!("{}.{}", self.market, self.code)
    }
    pub fn matches_url(&self, url: &url::Url, adjusted: bool) -> bool {
        let parameter = |key: &str, expected: &str| {
            url.query_pairs()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
                .collect::<Vec<_>>()
                == [expected]
        };
        url.scheme() == "https"
            && url.host_str() == Some("push2his.eastmoney.com")
            && url.path() == "/api/qt/stock/kline/get"
            && url.username().is_empty()
            && url.password().is_none()
            && parameter("secid", &self.secid())
            && parameter("klt", "101")
            && parameter("fqt", if adjusted { "1" } else { "0" })
    }
}
