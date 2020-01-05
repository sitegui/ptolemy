mod data_types;

use actix_web::{web, App, HttpServer, Responder};
use data_types::*;
use std::error::Error;

async fn route(info: web::Path<String>) -> impl Responder {
    ""
    // let coordinates: Coordinates = info.parse()?;
    // Ok(format!("Hello {:?}!", coordinates))
}

#[actix_rt::main]
pub async fn run_api() -> std::io::Result<()> {
    HttpServer::new(|| App::new().route("/route/v1/driving/{coordinates}", web::get().to(route)))
        .bind("127.0.0.1:8000")?
        .run()
        .await
}
