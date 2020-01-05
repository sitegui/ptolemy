mod data_types;

use crate::cartograph::*;
use actix_web::{get, web, App, HttpServer, Responder};
use data_types::*;
use std::path::Path;

#[get("/route/v1/driving/{coordinates}")]
async fn route(coords: web::Path<Coordinates>, carto: web::Data<Cartograph>) -> impl Responder {
    // Project the points
    let waypoints: Vec<_> = coords
        .0
        .iter()
        .map(|point| carto.project(point).unwrap())
        .collect();

    web::Json(RouteResponse {
        waypoints: waypoints
            .iter()
            .map(|waypoint| WaypointResponse {
                distance: waypoint.projected.haversine_distance(&waypoint.original),
                location: [
                    waypoint.projected.lon.as_degrees(),
                    waypoint.projected.lat.as_degrees(),
                ],
            })
            .collect(),
        routes: vec![],
    })
}

#[actix_rt::main]
pub async fn run_api<P: AsRef<Path> + 'static>(input: P) -> std::io::Result<()> {
    // Create a single instance of the cartography and wrap in an Data so that the threads
    // created by HttpServer::new can all have read access to it
    println!("will open carto");
    let carto = web::Data::new(Cartograph::open(input)?);
    println!("opened carto");
    HttpServer::new(move || App::new().app_data(carto.clone()).service(route))
        .bind("127.0.0.1:8000")?
        .run()
        .await
}
