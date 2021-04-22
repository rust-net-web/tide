use http_types::{self, Method, Url};
use tide::{self, Request, Response, Result};

#[async_std::test]
async fn test_missing_param() -> tide::Result<()> {
    async fn greet(req: Request<()>) -> Result<Response> {
        assert_eq!(req.param("name")?, "Param \"name\" not found");
        Ok(Response::new(200))
    }

    let mut server = tide::new();
    server.at("/").get(greet);

    let req = http_types::Request::new(Method::Get, Url::parse("http://example.com/")?);
    let res: http_types::Response = server.respond(req).await?;
    assert_eq!(res.status(), 500);
    Ok(())
}

#[async_std::test]
async fn hello_world_parametrized() -> Result<()> {
    async fn greet(req: tide::Request<()>) -> Result<impl Into<Response>> {
        let body = format!("{} says hello", req.param("name").unwrap_or("nori"));
        Ok(Response::builder(200).body(body))
    }

    let mut server = tide::new();
    server.at("/").get(greet);
    server.at("/:name").get(greet);

    let req = http_types::Request::new(Method::Get, Url::parse("http://example.com/")?);
    let mut res: http_types::Response = server.respond(req).await?;
    assert_eq!(res.body_string().await?, "nori says hello");

    let req = http_types::Request::new(Method::Get, Url::parse("http://example.com/iron")?);
    let mut res: http_types::Response = server.respond(req).await?;
    assert_eq!(res.body_string().await?, "iron says hello");

    let req = http_types::Request::new(Method::Get, Url::parse("http://example.com/nori%26iron")?);
    let mut res: http_types::Response = server.respond(req).await?;
    assert_eq!(res.body_string().await?, "nori&iron says hello");

    Ok(())
}

#[async_std::test]
async fn multi_segment_parameter() -> Result<()> {
    async fn greet(req: tide::Request<()>) -> Result<impl Into<Response>> {
        let body = req.param("param").unwrap();
        Ok(Response::builder(200).body(body))
    }

    let mut server = tide::new();
    server.at("/*param").get(greet);

    // Should still work with one segment
    let req = http_types::Request::new(Method::Get, Url::parse("http://example.com/one")?);
    let mut res: http_types::Response = server.respond(req).await?;
    assert_eq!(res.body_string().await?, "one");

    // Or two
    let req = http_types::Request::new(Method::Get, Url::parse("http://example.com/one/two")?);
    let mut res: http_types::Response = server.respond(req).await?;
    assert_eq!(res.body_string().await?, "one/two");

    // And it should percent-decode in between the slashes
    let req = http_types::Request::new(
        Method::Get,
        Url::parse("http://example.com/one/two%20three")?,
    );
    let mut res: http_types::Response = server.respond(req).await?;
    assert_eq!(res.body_string().await?, "one/two three");

    Ok(())
}
