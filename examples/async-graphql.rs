use async_graphql::http::{playground_source, receive_json, GraphQLPlaygroundConfig};
use async_graphql::{Context, EmptySubscription, InputObject, Object, Schema};
use async_std::sync::{Arc, RwLock};
use tide::http::mime;
use tide::{Body, Request, Response, Server};

#[derive(Clone)]
struct User {
    id: Option<u16>,
    first_name: String,
}

/// A user
#[Object]
impl User {
    /// A user id
    async fn id(&self) -> i32 {
        self.id.unwrap_or(0) as i32
    }

    /// A user first_name
    async fn first_name(&self) -> &str {
        &self.first_name
    }
}

#[derive(InputObject)]
struct NewUser {
    first_name: String,
}

impl NewUser {
    fn into_internal(self) -> User {
        User {
            id: None,
            first_name: self.first_name,
        }
    }
}

#[derive(Clone)]
pub struct State(Schema<QueryRoot, MutationRoot, EmptySubscription>);

#[derive(Default)]
pub struct Users(Arc<RwLock<Vec<User>>>);

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get all Users
    async fn users(&self, ctx: &Context<'_>) -> Vec<User> {
        let users = ctx.data_unchecked::<Users>().0.read().await;
        users.iter().cloned().collect()
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Add new user
    async fn add_user(&self, ctx: &Context<'_>, user: NewUser) -> User {
        let mut users = ctx.data_unchecked::<Users>().0.write().await;
        let mut user = user.into_internal();
        user.id = Some((users.len() + 1) as u16);
        users.push(user.clone());
        user
    }
}

async fn handle_graphql(req: Request<State>) -> tide::Result<impl Into<Response>> {
    let schema = req.state().0.clone();
    let resp = schema.execute(receive_json(req).await?).await;
    let mut http_resp = Response::new(http_types::StatusCode::Ok);
    http_resp.set_body(Body::from_json(&resp)?);
    Ok(http_resp)
}

async fn handle_playground(_: Request<State>) -> tide::Result<impl Into<Response>> {
    Ok(Response::builder(200)
        .body(playground_source(GraphQLPlaygroundConfig::new("/")))
        .content_type(mime::HTML))
}

#[async_std::main]
async fn main() -> std::io::Result<()> {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(Users::default())
        .finish();
    let mut app = Server::with_state(State(schema));
    app.at("/").post(handle_graphql);
    app.at("/").get(handle_playground);
    app.listen("0.0.0.0:8080").await?;
    Ok(())
}
