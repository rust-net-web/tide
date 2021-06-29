// WARNING: Don't not use it in production!
// This is just an example, so there is much more to a correct secure reverse proxy implementation.
//
// Example: HTTP GET to http://localhost:8080/http-rs/tide
// $ curl "http://localhost:8080/http-rs/tide"
// I'll show the Tide page at GitHub
#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    tide::log::start();
    let mut app = tide::new();
    app.at("*").get(|req: tide::Request<()>| async move {
        let url = "https://github.com"; // Change to your reverse proxy URL
        let mut req_builder = surf::get(format!("{}{}", url, req.url().path()));
        for (n, v) in req.iter().filter(|(n, _)| *n != "host") {
            let v: String = v.iter().map(|s| s.as_str()).collect();
            req_builder = req_builder.header(n, v);
        }
        let mut proxy_res = req_builder.send().await?;
        let mut res = tide::http::Response::new(proxy_res.status());
        proxy_res.iter().for_each(|(n, v)| {
            res.append_header(n, v);
        });
        if let Some(mime) = proxy_res.content_type() {
            res.set_content_type(mime);
        }
        res.set_body(proxy_res.take_body());
        Ok(res)
    });
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}
