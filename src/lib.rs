mod auth;
mod crypto;
mod db;
mod export;
mod models;
mod routes;

use worker::*;

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    routes::handle(req, env).await
}
