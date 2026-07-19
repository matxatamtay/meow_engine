//! Cookie ownership for the direct/network-process loader.

use http::{HeaderMap, header::SET_COOKIE};
use meow_url_policy::BrowserUrl;

#[derive(Clone, Debug)]
struct Cookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    same_site: SameSite,
}

#[derive(Clone, Copy, Debug, Default)]
enum SameSite {
    Strict,
    #[default]
    Lax,
    None,
}

#[derive(Debug, Default)]
pub(crate) struct CookieJar {
    cookies: Vec<Cookie>,
}

impl CookieJar {
    pub(crate) fn store_response(&mut self, url: &BrowserUrl, headers: &HeaderMap) {
        for value in headers.get_all(SET_COOKIE) {
            let Ok(value) = value.to_str() else {
                continue;
            };
            if let Some(cookie) = parse_cookie(url, value) {
                self.cookies.retain(|existing| {
                    !(existing.name == cookie.name
                        && existing.domain == cookie.domain
                        && existing.path == cookie.path)
                });
                if !cookie.value.is_empty() {
                    self.cookies.push(cookie);
                }
            }
        }
    }

    pub(crate) fn header_for(&self, target: &BrowserUrl, document: &BrowserUrl) -> Option<String> {
        let host = target.as_url().host_str()?;
        let path = target.as_url().path();
        let secure = matches!(target.scheme(), "https" | "wss");
        let same_site = is_same_site(target, document);
        let pairs = self
            .cookies
            .iter()
            .filter(|cookie| domain_matches(host, &cookie.domain))
            .filter(|cookie| path.starts_with(&cookie.path))
            .filter(|cookie| !cookie.secure || secure)
            .filter(|cookie| {
                same_site || matches!(cookie.same_site, SameSite::None) && cookie.secure
            })
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    }
}

fn parse_cookie(url: &BrowserUrl, value: &str) -> Option<Cookie> {
    let mut parts = value.split(';');
    let (name, cookie_value) = parts.next()?.split_once('=')?;
    let host = url.as_url().host_str()?.to_ascii_lowercase();
    let mut cookie = Cookie {
        name: name.trim().to_owned(),
        value: cookie_value.trim().to_owned(),
        domain: host.clone(),
        path: default_cookie_path(url.as_url().path()),
        secure: false,
        same_site: SameSite::Lax,
    };
    if cookie.name.is_empty() {
        return None;
    }
    for attribute in parts {
        let attribute = attribute.trim();
        let (name, value) = attribute
            .split_once('=')
            .map_or((attribute, ""), |(name, value)| (name, value));
        match name.trim().to_ascii_lowercase().as_str() {
            "domain" => {
                let domain = value.trim().trim_start_matches('.').to_ascii_lowercase();
                if !domain_matches(&host, &domain) {
                    return None;
                }
                cookie.domain = domain;
            }
            "path" if value.starts_with('/') => cookie.path = value.to_owned(),
            "secure" => cookie.secure = true,
            "httponly" => {}
            "samesite" => {
                cookie.same_site = match value.trim().to_ascii_lowercase().as_str() {
                    "strict" => SameSite::Strict,
                    "none" => SameSite::None,
                    _ => SameSite::Lax,
                };
            }
            "max-age" if value.trim().parse::<i64>().ok().is_some_and(|age| age <= 0) => {
                cookie.value.clear();
            }
            _ => {}
        }
    }
    if matches!(cookie.same_site, SameSite::None) && !cookie.secure {
        return None;
    }
    Some(cookie)
}

fn default_cookie_path(path: &str) -> String {
    if !path.starts_with('/') || path == "/" {
        return "/".to_owned();
    }
    path.rsplit_once('/')
        .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
        .unwrap_or("/")
        .to_owned()
}

fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn is_same_site(left: &BrowserUrl, right: &BrowserUrl) -> bool {
    left.scheme() == right.scheme()
        && site_key(left.as_url().host_str()) == site_key(right.as_url().host_str())
}

fn site_key(host: Option<&str>) -> Option<String> {
    let host = host?;
    let labels = host.split('.').collect::<Vec<_>>();
    if labels.len() < 2 {
        Some(host.to_ascii_lowercase())
    } else {
        Some(format!(
            "{}.{}",
            labels[labels.len() - 2],
            labels[labels.len() - 1]
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_site_and_secure_rules_are_applied() {
        let url = BrowserUrl::parse("https://api.example.test/path/index").unwrap();
        let mut jar = CookieJar::default();
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            "strict=1; SameSite=Strict; Secure".parse().unwrap(),
        );
        headers.append(SET_COOKIE, "none=1; SameSite=None; Secure".parse().unwrap());
        jar.store_response(&url, &headers);
        assert_eq!(
            jar.header_for(
                &url,
                &BrowserUrl::parse("https://www.example.test/").unwrap()
            )
            .as_deref(),
            Some("strict=1; none=1")
        );
        assert_eq!(
            jar.header_for(&url, &BrowserUrl::parse("https://other.test/").unwrap())
                .as_deref(),
            Some("none=1")
        );
    }
}
