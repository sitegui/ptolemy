pub mod graph;
pub mod junction;
pub mod node;
pub mod serialize;

use osmpbf::Way;

/// Detect whether a given node is a barrier
pub fn parse_barrier<'a, I: Iterator<Item = (&'a str, &'a str)>>(mut node_tags: I) -> bool {
    node_tags
        .find(|tag| tag.0 == "barrier")
        .map(|tag| match tag.1 {
            "border_control" | "block" | "bollard" | "chain" | "debris" | "gate"
            | "jersey_barrier" | "kent_carriage_gap" => true,
            _ => false,
        })
        .unwrap_or(false)
}

/// Convert the value of the tag `highway` to a `road_level` (from 0 to 5)
pub fn parse_road_level(way: &Way) -> Option<u8> {
    get_tag(way, "highway").and_then(|value| match value {
        "motorway" => Some(0),
        "motorway_link" => Some(0),
        "trunk" => Some(0),
        "trunk_link" => Some(0),
        "primary" => Some(1),
        "primary_link" => Some(1),
        "secondary" => Some(2),
        "secondary_link" => Some(2),
        "tertiary" => Some(3),
        "tertiary_link" => Some(3),
        "unclassified" => Some(4),
        "residential" => Some(5),
        "service" => Some(5),
        "living_street" => Some(5),
        "road" => Some(5),
        "rest_area" => Some(5),
        "services" => Some(5),
        _ => None,
    })
}

pub struct Direction {
    pub direct: bool,
    pub reverse: bool,
}

/// Convert the value of the tag `oneway`
pub fn parse_oneway(way: &Way) -> Direction {
    let junction = get_tag(way, "junction");
    let highway = get_tag(way, "highway");
    let oneway = get_tag(way, "oneway");

    // As per https://wiki.openstreetmap.org/wiki/Key:oneway, for roundabouts and motorways,
    // ways are single-way by default, unless explicitly stated otherwise
    let implied = if junction == Some("roundabout") || highway == Some("motorway") {
        (true, false)
    } else {
        (true, true)
    };

    let (direct, reverse) = match oneway {
        Some("yes") | Some("true") | Some("1") => (true, false),
        Some("no") | Some("false") | Some("0") => (true, false),
        Some("-1") => (false, true),
        _ => implied,
    };

    Direction { direct, reverse }
}

fn get_tag<'a>(way: &'a Way, name: &'_ str) -> Option<&'a str> {
    way.tags().find(|tag| tag.0 == name).map(|tag| tag.1)
}
