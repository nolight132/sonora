use anyhow::{Context as _, Result, bail};
use ytmusic::browser::{self, Browser, Family};

const PROOF: &[&str] = &["SAPISID", "__Secure-3PAPISID"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieAuth {
    pub cookies: String,
    pub authuser: Option<usize>,
    pub page_id: Option<String>,
}

pub fn cookies(browser: &Browser) -> Result<String> {
    if browser.family != Family::Firefox {
        bail!("{} is not a firefox-based browser", browser.name);
    }
    browser::cookies(browser).with_context(|| format!("cannot read cookies from {}", browser.name))
}

pub fn header(input: &str) -> Result<CookieAuth> {
    if input.trim().is_empty() {
        bail!("cookie header is empty");
    }
    let raw = request_header(input, "cookie").unwrap_or_else(|| input.trim());
    let pairs: Vec<&str> = raw
        .split(';')
        .map(str::trim)
        .filter(|pair| pair.contains('=') && !pair.contains(char::is_whitespace))
        .collect();
    let signed_in = pairs
        .iter()
        .filter_map(|pair| pair.split_once('='))
        .any(|(name, _)| PROOF.contains(&name));
    if !signed_in {
        bail!(
            "the pasted text carries no SAPISID or __Secure-3PAPISID; copy the whole value of the Cookie request header, not the request Cookies panel"
        );
    }
    let authuser = request_header(input, "x-goog-authuser")
        .map(|value| {
            value
                .parse::<usize>()
                .context("X-Goog-AuthUser is not a number")
        })
        .transpose()?;
    let page_id = request_header(input, "x-goog-pageid")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                bail!("X-Goog-PageId is not a numeric Brand Account ID");
            }
            Ok(value.to_string())
        })
        .transpose()?;
    Ok(CookieAuth {
        cookies: pairs.join("; "),
        authuser,
        page_id,
    })
}

fn request_header<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    request_header_with_colon(input, name).or_else(|| request_header_without_colon(input, name))
}

fn request_header_with_colon<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    let bytes = input.as_bytes();
    let needle = name.as_bytes();
    for start in 0..bytes.len().saturating_sub(needle.len()) + 1 {
        let end = start + needle.len();
        let boundary = start == 0 || bytes[start - 1].is_ascii_whitespace();
        if !boundary
            || !bytes[start..end].eq_ignore_ascii_case(needle)
            || bytes.get(end) != Some(&b':')
        {
            continue;
        }
        let value_start = end + 1;
        let value_end = next_header(input, value_start).unwrap_or(input.len());
        return Some(input[value_start..value_end].trim());
    }
    None
}

fn request_header_without_colon<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    const REQUEST_HEADERS: &[&str] = &[
        "accept",
        "accept-encoding",
        "accept-language",
        "authorization",
        "content-encoding",
        "content-length",
        "content-type",
        "cookie",
        "origin",
        "priority",
        "referer",
        "sec-ch-ua",
        "sec-ch-ua-arch",
        "sec-ch-ua-bitness",
        "sec-ch-ua-full-version-list",
        "sec-ch-ua-mobile",
        "sec-ch-ua-model",
        "sec-ch-ua-platform",
        "sec-ch-ua-platform-version",
        "sec-ch-ua-wow64",
        "sec-fetch-dest",
        "sec-fetch-mode",
        "sec-fetch-site",
        "sec-gpc",
        "user-agent",
        "x-goog-authuser",
        "x-goog-pageid",
        "x-goog-visitor-id",
        "x-origin",
        "x-youtube-bootstrap-logged-in",
        "x-youtube-client-name",
        "x-youtube-client-version",
    ];

    let (value_start, _) = bare_header_at(input, name, 0)?;
    let value_end = REQUEST_HEADERS
        .iter()
        .filter_map(|candidate| {
            bare_header_at(input, candidate, value_start).map(|(_, start)| start)
        })
        .min()
        .unwrap_or(input.len());
    Some(input[value_start..value_end].trim())
}

fn bare_header_at(input: &str, name: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = input.as_bytes();
    let needle = name.as_bytes();
    let mut start = from;
    while start + needle.len() <= bytes.len() {
        let end = start + needle.len();
        let left_boundary = start == 0 || bytes[start - 1].is_ascii_whitespace();
        let right_boundary = bytes.get(end).is_some_and(u8::is_ascii_whitespace);
        if left_boundary && right_boundary && bytes[start..end].eq_ignore_ascii_case(needle) {
            let value_start = bytes[end..]
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .map(|offset| end + offset)?;
            return Some((value_start, start));
        }
        start += 1;
    }
    None
}

fn next_header(input: &str, value_start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut index = value_start;
    while index < bytes.len() {
        if !bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let boundary = index;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let name_start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'-')
        {
            index += 1;
        }
        if index > name_start && bytes.get(index) == Some(&b':') {
            return Some(boundary);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::header;

    #[test]
    fn keeps_a_signed_in_header() {
        let value = header("VISITOR_INFO1_LIVE=abc;  SAPISID=xyz; PREF=f1").unwrap();
        assert_eq!(
            value.cookies,
            "VISITOR_INFO1_LIVE=abc; SAPISID=xyz; PREF=f1"
        );
    }

    #[test]
    fn drops_the_header_name() {
        let value = header("Cookie: SAPISID=xyz; SID=def").unwrap();
        assert_eq!(value.cookies, "SAPISID=xyz; SID=def");
    }

    #[test]
    fn picks_the_cookie_line_out_of_a_blob() {
        let blob = "POST /youtubei/v1/browse HTTP/2\nHost: music.youtube.com\ncookie: SAPISID=abc; SID=def\nOrigin: https://music.youtube.com";
        assert_eq!(header(blob).unwrap().cookies, "SAPISID=abc; SID=def");
    }

    #[test]
    fn accepts_the_secure_variant_alone() {
        assert!(header("__Secure-3PAPISID=xyz").is_ok());
    }

    #[test]
    fn rejects_a_signed_out_header() {
        assert!(header("VISITOR_INFO1_LIVE=abc; PREF=f1").is_err());
    }

    #[test]
    fn rejects_an_empty_paste() {
        assert!(header("   \n ").is_err());
    }

    #[test]
    fn rejects_a_bare_cookie_name() {
        assert!(header("SAPISID").is_err());
    }

    #[test]
    fn reads_brand_account_request_headers() {
        let input = "Cookie: SAPISID=abc; SID=def\nX-Goog-AuthUser: 2\nx-goog-pageid: 101234161234936123473";
        let value = header(input).unwrap();
        assert_eq!(value.cookies, "SAPISID=abc; SID=def");
        assert_eq!(value.authuser, Some(2));
        assert_eq!(value.page_id.as_deref(), Some("101234161234936123473"));
    }

    #[test]
    fn reads_request_headers_flattened_by_the_login_field() {
        let input = "Accept: */* Cookie: SAPISID=abc; SID=def X-Goog-AuthUser: 2 X-Goog-PageId: 101234161234936123473 X-Origin: https://music.youtube.com";
        let value = header(input).unwrap();
        assert_eq!(value.cookies, "SAPISID=abc; SID=def");
        assert_eq!(value.authuser, Some(2));
        assert_eq!(value.page_id.as_deref(), Some("101234161234936123473"));
    }

    #[test]
    fn reads_chromium_copied_headers_flattened_by_the_login_field() {
        let input = "accept */* cookie YSC=visitor; SAPISID=abc; SID=def origin https://music.youtube.com x-goog-authuser 0 x-goog-pageid 123456789012345678901 x-origin https://music.youtube.com";
        let value = header(input).unwrap();
        assert_eq!(value.cookies, "YSC=visitor; SAPISID=abc; SID=def");
        assert_eq!(value.authuser, Some(0));
        assert_eq!(value.page_id.as_deref(), Some("123456789012345678901"));
    }

    #[test]
    fn rejects_an_invalid_brand_account_id() {
        let input = "Cookie: SAPISID=abc\nX-Goog-PageId: 123 bad";
        assert!(header(input).is_err());
    }
}
