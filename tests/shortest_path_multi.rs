use ptolemy::*;

#[test]
#[ignore]
fn shortest_path() {
    let carto = Cartograph::open("test_data/andorra.ptolemy").unwrap();
    let from = carto.project(&GeoPoint::from_degrees(42.553210, 1.588908));
    let to1 = carto.project(&GeoPoint::from_degrees(42.564440, 1.685042));
    let to2 = carto.project(&GeoPoint::from_degrees(42.440226, 1.492084));
    let to3 = carto.project(&GeoPoint::from_degrees(42.500441, 1.519031));
    let mut to = vec![to1, to2, to3];
    for delta in 0..100 {
        let delta = delta as f64 / 1e4;
        to.push(carto.project(&GeoPoint::from_degrees(42.500441 + delta, 1.519031 + delta)));
    }
    let single_distances: Vec<u32> = to
        .iter()
        .map(|to| carto.shortest_path(&from, to).distance)
        .collect();

    let mut timer = DebugTime::new();
    for _ in 0..1_000 {
        for (to, &dist) in to.iter().zip(single_distances.iter()) {
            assert_eq!(carto.shortest_path(&from, to).distance, dist);
        }
    }
    timer.msg("single");

    let mut timer = DebugTime::new();
    for _ in 0..1_000 {
        assert_eq!(carto.shortest_path_multi(&from, &to), single_distances);
    }
    timer.msg("multi");
}
