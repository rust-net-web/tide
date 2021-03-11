mod test_utils;
use test_utils::ServerTestingExt;

use cookie::SameSite;
use http_types::headers::SET_COOKIE;
use std::time::Duration;
use tide::{
    http::{cookies as cookie, headers::HeaderValue, Response},
    sessions::{MemoryStore, SessionMiddleware},
    utils::Before,
};
#[derive(Clone, Debug, Default, PartialEq)]
struct SessionData {
    visits: usize,
}

#[async_std::test]
async fn test_basic_sessions() -> tide::Result<()> {
    let mut app = tide::new();
    app.with(SessionMiddleware::new(
        MemoryStore::new(),
        b"12345678901234567890123456789012345",
    ));

    app.with(Before(|mut request: tide::Request<()>| async move {
        let visits: usize = request.session().get("visits").unwrap_or_default();
        request.session_mut().insert("visits", visits + 1).unwrap();
        request
    }));

    app.at("/").get(|req: tide::Request<()>| async move {
        let visits: usize = req.session().get("visits").unwrap();
        Ok(format!("you have visited this website {} times", visits))
    });

    let response = app.get("/").await?;
    let cookies = Cookies::from_response(&response);
    let cookie = &cookies["tide.sid"];
    assert_eq!(cookie.name(), "tide.sid");
    assert_eq!(cookie.domain(), None);
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(cookie.same_site(), Some(SameSite::Strict));
    assert_eq!(cookie.secure(), None); // this request was http://
    assert_eq!(cookie.path(), Some("/"));

    let mut second_response = app.get("/").header("Cookie", &cookies).await?;

    let body = second_response.body_string().await?;
    assert_eq!("you have visited this website 2 times", body);
    assert!(second_response.header("Set-Cookie").is_none());

    let response = app.get("https://secure/").await?;
    let cookies = Cookies::from_response(&response);
    let cookie = &cookies["tide.sid"];
    assert_eq!(cookie.secure(), Some(true));
    Ok(())
}

#[async_std::test]
async fn test_customized_sessions() -> tide::Result<()> {
    let mut app = tide::new();
    app.with(
        SessionMiddleware::new(MemoryStore::new(), b"12345678901234567890123456789012345")
            .with_cookie_name("custom.cookie.name")
            .with_cookie_path("/nested")
            .with_cookie_domain("www.rust-lang.org")
            .with_secure(true)
            .with_same_site_policy(SameSite::Lax)
            .with_session_ttl(Some(Duration::from_secs(1)))
            .without_save_unchanged(),
    );

    app.at("/").get(|_| async { Ok("/") });
    app.at("/nested").get(|req: tide::Request<()>| async move {
        Ok(format!(
            "/nested {}",
            req.session().get::<usize>("visits").unwrap_or_default()
        ))
    });
    app.at("/nested/incr")
        .get(|mut req: tide::Request<()>| async move {
            let mut visits: usize = req.session().get("visits").unwrap_or_default();
            visits += 1;
            req.session_mut().insert("visits", visits)?;
            Ok(format!("/nested/incr {}", visits))
        });

    let response = app.get("/").await?;
    assert_eq!(Cookies::from_response(&response).len(), 0);

    let mut response = app.get("/nested").await?;
    assert_eq!(Cookies::from_response(&response).len(), 0);
    assert_eq!(response.body_string().await?, "/nested 0");

    let mut response = app.get("/nested/incr").await?;
    let cookies = Cookies::from_response(&response);
    assert_eq!(response.body_string().await?, "/nested/incr 1");

    assert_eq!(cookies.len(), 1);
    assert!(cookies.get("tide.sid").is_none());
    let cookie = &cookies["custom.cookie.name"];
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(cookie.secure(), Some(true));
    assert_eq!(cookie.same_site(), Some(SameSite::Lax));
    assert_eq!(cookie.path(), Some("/nested"));
    assert_eq!(cookie.domain(), Some("www.rust-lang.org"));
    let cookie_value = cookie.value().to_string();

    let mut second_response = app
        .get("https://whatever/nested/incr")
        .header("Cookie", &cookies)
        .await?;
    let body = second_response.body_string().await?;
    assert_eq!("/nested/incr 2", body);
    assert!(second_response.header("Set-Cookie").is_none());

    async_std::task::sleep(Duration::from_secs(5)).await; // wait for expiration

    let mut expired_response = app
        .get("https://whatever/nested/incr")
        .header("Cookie", &cookies)
        .await?;
    let cookies = Cookies::from_response(&expired_response);
    assert_eq!(cookies.len(), 1);
    assert!(cookies["custom.cookie.name"].value() != cookie_value);

    let body = expired_response.body_string().await?;
    assert_eq!("/nested/incr 1", body);
    Ok(())
}

#[async_std::test]
async fn test_session_destruction() -> tide::Result<()> {
    let mut app = tide::new();
    app.with(SessionMiddleware::new(
        MemoryStore::new(),
        b"12345678901234567890123456789012345",
    ));

    app.with(Before(|mut request: tide::Request<()>| async move {
        let visits: usize = request.session().get("visits").unwrap_or_default();
        request.session_mut().insert("visits", visits + 1).unwrap();
        request
    }));

    app.at("/").get(|req: tide::Request<()>| async move {
        let visits: usize = req.session().get("visits").unwrap();
        Ok(format!("you have visited this website {} times", visits))
    });

    app.at("/logout")
        .post(|mut req: tide::Request<()>| async move {
            req.session_mut().destroy();
            Ok(Response::new(200))
        });

    let response = app.get("/").await?;
    let cookies = Cookies::from_response(&response);

    let second_response = app
        .post("https://whatever/logout")
        .header("Cookie", &cookies)
        .await?;
    let cookies = Cookies::from_response(&second_response);
    assert_eq!(cookies["tide.sid"].value(), "");
    assert_eq!(cookies.len(), 1);
    Ok(())
}

#[derive(Debug, Clone)]
struct Cookies(Vec<tide::http::Cookie<'static>>);
impl Cookies {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn from_response(response: &impl AsRef<http_types::Response>) -> Self {
        response
            .as_ref()
            .header(SET_COOKIE)
            .map(|hv| hv.to_string())
            .unwrap_or_else(|| "[]".into())
            .parse()
            .unwrap()
    }

    fn get<'a>(&'a self, name: &str) -> Option<&'a tide::http::Cookie<'static>> {
        self.0.iter().find(|cookie| cookie.name() == name)
    }
}
impl tide::http::headers::ToHeaderValues for &Cookies {
    type Iter = std::iter::Once<HeaderValue>;
    fn to_header_values(&self) -> http_types::Result<Self::Iter> {
        let value = self
            .0
            .iter()
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect::<Vec<_>>()
            .join("; ");
        Ok(std::iter::once(HeaderValue::from_bytes(value.into())?))
    }
}

impl std::ops::Index<&str> for Cookies {
    type Output = cookie::Cookie<'static>;
    fn index(&self, index: &str) -> &Self::Output {
        self.get(index).unwrap()
    }
}

impl std::str::FromStr for Cookies {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let strings: Vec<String> = serde_json::from_str(s).unwrap_or_default();

        Ok(Self(
            strings
                .iter()
                .filter_map(|cookie| cookie::Cookie::parse(cookie.to_owned()).ok())
                .collect(),
        ))
    }
}
