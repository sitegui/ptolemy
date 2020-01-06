use ptolemy::*;

#[test]
fn shortest_path() {
    let carto = Cartograph::open("test_data/andorra.ptolemy").unwrap();

    for _ in 0..100_000 {
        let p1 = carto
            .project(&GeoPoint::from_degrees(42.509827, 1.537439))
            .unwrap();
        let p2 = carto
            .project(&GeoPoint::from_degrees(42.438849, 1.491521))
            .unwrap();
        let path = carto.shortest_path(&p1, &p2).unwrap();
        assert_eq!(path.distance, 13474);
    }
}
