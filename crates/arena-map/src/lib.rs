//! Map files: one JSON document per arena, the single source of truth for the
//! rules' layout and the venue's look. Distances in the file are metres; the
//! rules receive integer millimetres. Parsing and validation are pure, so the
//! server, the browser client and tests all load maps identically.
//!
//! Gameplay checks live with the rules (`Layout::validate`); this crate adds
//! the presentation checks, such as starting-room ceilings at least three
//! character heights tall.

use arena_sim::arena::Position;
use arena_sim::map::{Block, Edge, Layout, Room, Spawn};
use serde::{Deserialize, Serialize};

/// The file format this crate reads and writes.
pub const FORMAT: u32 = 1;
/// Shared character height (metres), matching the client's normalized models.
pub const CHARACTER_HEIGHT: f32 = 1.9;
/// Starting rooms must be at least three characters tall.
pub const MIN_ROOM_CEILING: f32 = 3.0 * CHARACTER_HEIGHT;
/// Highest reachable orbit-camera eye (metres): pivot plus jump apex plus the
/// longest zoom at the steepest pitch. Pillars and the building must clear it.
pub const HIGHEST_EYE: f32 = 23.0;
/// Room for hanging banners between the concourse and the outer wall's top.
const BANNER_SPACE: f32 = 12.0;

/// The default arena, embedded so the rules, server and tests always have a map.
pub const COLOSSEUM_JSON: &str = include_str!("../../../assets/maps/colosseum.json");

/// Venue settings (metres) that shape the stands and building around the floor.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Venue {
    pub pit_wall_height: f32,
    pub pit_wall_thickness: f32,
    pub tiers: u32,
    pub tier_rise: f32,
    pub tier_tread: f32,
    pub concourse_depth: f32,
    pub outer_wall_thickness: f32,
    pub outer_wall_height: f32,
    pub pillar_height: f32,
}
impl Venue {
    /// Distance from the floor edge to the back of the top tier.
    pub fn stands_depth(&self) -> f32 {
        self.pit_wall_thickness + self.tiers as f32 * self.tier_tread
    }
    /// Height of the concourse behind the top tier.
    pub fn concourse_height(&self) -> f32 {
        self.pit_wall_height + self.tiers as f32 * self.tier_rise
    }
    /// Inner face of the outer wall, measured from the floor edge.
    pub fn wall_inner(&self) -> f32 {
        self.stands_depth() + self.concourse_depth
    }
    /// Outer face of the building, measured from the floor edge.
    pub fn footprint(&self) -> f32 {
        self.wall_inner() + self.outer_wall_thickness
    }
}

/// Presentation settings for one room, parallel to `Layout::rooms`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomLook {
    pub ceiling: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapFile {
    pub name: String,
    pub layout: Layout,
    pub rooms: Vec<RoomLook>,
    pub venue: Venue,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileJson {
    format: u32,
    name: String,
    floor: FloorJson,
    pillars: Vec<PillarJson>,
    rooms: Vec<RoomJson>,
    venue: Venue,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FloorJson {
    width: f64,
    depth: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PillarJson {
    x: f64,
    z: f64,
    width: f64,
    depth: f64,
}
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum TeamJson {
    Friendly,
    Opposing,
}
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum EdgeJson {
    North,
    South,
    West,
    East,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomJson {
    team: TeamJson,
    edge: EdgeJson,
    offset: f64,
    width: f64,
    depth: f64,
    ceiling: f32,
    spawns: Vec<SpawnJson>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpawnJson {
    across: f64,
    back: f64,
}

/// Metres to whole millimetres, refusing values no arena could use.
fn mm(value: f64, what: &str, errors: &mut Vec<String>) -> i32 {
    if !value.is_finite() || value.abs() > 1_000.0 {
        errors.push(format!("{what} must be a number within ±1000 m."));
        return 0;
    }
    (value * 1000.0).round() as i32
}

fn metres(mm: i32) -> f64 {
    f64::from(mm) / 1000.0
}

/// Parses and validates a map file. Every problem is reported, in plain
/// language, so an editor can show them all at once.
pub fn parse(json: &str) -> Result<MapFile, Vec<String>> {
    let file: FileJson = serde_json::from_str(json)
        .map_err(|error| vec![format!("Map file is malformed: {error}")])?;
    if file.format != FORMAT {
        return Err(vec![format!(
            "Map format {} is not supported (expected {FORMAT}).",
            file.format
        )]);
    }
    let mut errors = Vec::new();
    let half = |value: f64, what: &str, errors: &mut Vec<String>| mm(value, what, errors) / 2;
    let layout = Layout {
        half_x: half(file.floor.width, "Floor width", &mut errors),
        half_z: half(file.floor.depth, "Floor depth", &mut errors),
        pillars: file
            .pillars
            .iter()
            .map(|p| Block {
                center: Position {
                    x: mm(p.x, "Pillar x", &mut errors),
                    z: mm(p.z, "Pillar z", &mut errors),
                },
                half_x: half(p.width, "Pillar width", &mut errors),
                half_z: half(p.depth, "Pillar depth", &mut errors),
            })
            .collect(),
        rooms: file
            .rooms
            .iter()
            .map(|room| Room {
                team: match room.team {
                    TeamJson::Friendly => 0,
                    TeamJson::Opposing => 1,
                },
                edge: match room.edge {
                    EdgeJson::North => Edge::North,
                    EdgeJson::South => Edge::South,
                    EdgeJson::West => Edge::West,
                    EdgeJson::East => Edge::East,
                },
                offset: mm(room.offset, "Room offset", &mut errors),
                width: mm(room.width, "Room width", &mut errors),
                depth: mm(room.depth, "Room depth", &mut errors),
                spawns: room
                    .spawns
                    .iter()
                    .map(|s| Spawn {
                        across: mm(s.across, "Spawn across", &mut errors),
                        back: mm(s.back, "Spawn back", &mut errors),
                    })
                    .collect(),
            })
            .collect(),
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    let map = MapFile {
        name: file.name,
        layout,
        rooms: file
            .rooms
            .iter()
            .map(|room| RoomLook {
                ceiling: room.ceiling,
            })
            .collect(),
        venue: file.venue,
    };
    let mut errors = map.layout.validate().err().unwrap_or_default();
    errors.extend(map.check_venue());
    if map.name.trim().is_empty() {
        errors.push("The map needs a name.".into());
    }
    if errors.is_empty() {
        Ok(map)
    } else {
        Err(errors)
    }
}

impl MapFile {
    fn check_venue(&self) -> Vec<String> {
        let v = &self.venue;
        let mut errors = Vec::new();
        let ranges: [(&str, f32, f32, f32); 8] = [
            ("Pit wall height", v.pit_wall_height, 2.0, 30.0),
            ("Pit wall thickness", v.pit_wall_thickness, 0.5, 5.0),
            ("Tier rise", v.tier_rise, 0.3, 3.0),
            ("Tier tread", v.tier_tread, 0.8, 5.0),
            ("Concourse depth", v.concourse_depth, 1.0, 20.0),
            ("Outer wall thickness", v.outer_wall_thickness, 1.0, 10.0),
            ("Outer wall height", v.outer_wall_height, 10.0, 120.0),
            ("Pillar height", v.pillar_height, 2.0, 150.0),
        ];
        for (what, value, low, high) in ranges {
            if !(low..=high).contains(&value) {
                errors.push(format!("{what} must be {low}–{high} m (is {value} m)."));
            }
        }
        if !(1..=30).contains(&v.tiers) {
            errors.push(format!("Tiers must be 1–30 (is {}).", v.tiers));
        }
        if !errors.is_empty() {
            return errors;
        }
        if v.pillar_height < HIGHEST_EYE + 10.0 {
            errors.push(format!(
                "Pillars must be at least {} m tall so the camera never sees over them.",
                HIGHEST_EYE + 10.0
            ));
        }
        let outer = (HIGHEST_EYE + 5.0).max(v.concourse_height() + BANNER_SPACE);
        if v.outer_wall_height < outer {
            errors.push(format!(
                "The outer wall must be at least {outer:.1} m tall to hide the outside from the camera and fit the banners."
            ));
        }
        for (i, (room, look)) in self.layout.rooms.iter().zip(&self.rooms).enumerate() {
            let n = i + 1;
            if !look.ceiling.is_finite() || look.ceiling < MIN_ROOM_CEILING {
                errors.push(format!(
                    "Room {n}'s ceiling must be at least {MIN_ROOM_CEILING:.1} m (three character heights)."
                ));
            }
            if look.ceiling > v.pit_wall_height - 0.8 {
                errors.push(format!(
                    "Room {n}'s ceiling must be at least 0.8 m below the pit wall's top; raise the pit wall to {:.1} m or more.",
                    look.ceiling + 0.8
                ));
            }
            let depth = metres(room.depth) as f32;
            if depth < v.pit_wall_thickness + 2.0 {
                errors.push(format!(
                    "Room {n} must reach at least 2 m behind the pit wall."
                ));
            }
            if depth > v.wall_inner() {
                errors.push(format!(
                    "Room {n} is deeper than the stands and concourse ({:.1} m) and would break through the outer wall.",
                    v.wall_inner()
                ));
            }
        }
        errors
    }

    /// Serializes back to the file format (metres), for round trips and tools.
    pub fn to_json(&self) -> String {
        let file = FileJson {
            format: FORMAT,
            name: self.name.clone(),
            floor: FloorJson {
                width: metres(self.layout.half_x * 2),
                depth: metres(self.layout.half_z * 2),
            },
            pillars: self
                .layout
                .pillars
                .iter()
                .map(|p| PillarJson {
                    x: metres(p.center.x),
                    z: metres(p.center.z),
                    width: metres(p.half_x * 2),
                    depth: metres(p.half_z * 2),
                })
                .collect(),
            rooms: self
                .layout
                .rooms
                .iter()
                .zip(&self.rooms)
                .map(|(room, look)| RoomJson {
                    team: if room.team == 0 {
                        TeamJson::Friendly
                    } else {
                        TeamJson::Opposing
                    },
                    edge: match room.edge {
                        Edge::North => EdgeJson::North,
                        Edge::South => EdgeJson::South,
                        Edge::West => EdgeJson::West,
                        Edge::East => EdgeJson::East,
                    },
                    offset: metres(room.offset),
                    width: metres(room.width),
                    depth: metres(room.depth),
                    ceiling: look.ceiling,
                    spawns: room
                        .spawns
                        .iter()
                        .map(|s| SpawnJson {
                            across: metres(s.across),
                            back: metres(s.back),
                        })
                        .collect(),
                })
                .collect(),
            venue: self.venue,
        };
        serde_json::to_string_pretty(&file).expect("map files always serialize")
    }
}

/// The embedded default arena. It is validated by tests, so this cannot fail.
pub fn colosseum() -> MapFile {
    parse(COLOSSEUM_JSON).expect("the embedded colosseum map is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edited(edit: impl FnOnce(&mut serde_json::Value)) -> Result<MapFile, Vec<String>> {
        let mut value: serde_json::Value = serde_json::from_str(COLOSSEUM_JSON).unwrap();
        edit(&mut value);
        parse(&value.to_string())
    }

    #[test]
    fn colosseum_is_valid_and_round_trips() {
        let map = colosseum();
        assert_eq!(map.layout.half_x, 54_000);
        assert_eq!(map.layout.rooms.len(), 2);
        assert_eq!(parse(&map.to_json()), Ok(map.clone()));
        for look in &map.rooms {
            assert!(look.ceiling >= MIN_ROOM_CEILING);
        }
    }

    /// Maps saved by the editor land in assets/maps; every one must load.
    #[test]
    fn every_tracked_map_file_is_valid_and_canonical() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/maps");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "json") {
                let text = std::fs::read_to_string(&path).unwrap();
                let map = parse(&text).unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
                assert_eq!(parse(&map.to_json()), Ok(map), "{}", path.display());
                checked += 1;
            }
        }
        assert!(checked >= 1);
    }

    #[test]
    fn rooms_must_be_three_characters_tall_under_a_taller_pit_wall() {
        let low = edited(|v| v["rooms"][0]["ceiling"] = 5.0.into()).unwrap_err();
        assert!(
            low.join(" ").contains("5.7 m (three character heights)"),
            "{low:?}"
        );
        let wall = edited(|v| v["venue"]["pitWallHeight"] = 6.0.into()).unwrap_err();
        assert!(wall.join(" ").contains("raise the pit wall"), "{wall:?}");
    }

    #[test]
    fn venue_and_file_problems_are_reported_plainly() {
        let deep = edited(|v| v["rooms"][1]["depth"] = 30.into()).unwrap_err();
        assert!(deep.join(" ").contains("break through the outer wall"));
        let short = edited(|v| v["venue"]["pillarHeight"] = 20.into()).unwrap_err();
        assert!(short.join(" ").contains("camera never sees over"));
        let outer = edited(|v| v["venue"]["outerWallHeight"] = 20.into()).unwrap_err();
        assert!(outer.join(" ").contains("outer wall must be at least"));
        let unknown = edited(|v| v["floor"]["height"] = 3.into()).unwrap_err();
        assert!(unknown[0].contains("malformed"));
        let format = edited(|v| v["format"] = 2.into()).unwrap_err();
        assert!(format[0].contains("format 2"));
        let nan = parse(&COLOSSEUM_JSON.replace("\"offset\": 0,", "\"offset\": 1e9,")).unwrap_err();
        assert!(nan.join(" ").contains("±1000 m"));
        // Gameplay problems come through from the rules' own checks.
        let spawns = edited(|v| v["rooms"][0]["spawns"] = serde_json::json!([])).unwrap_err();
        assert!(spawns.join(" ").contains("Team 1 needs"));
    }
}
