use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

type LevelMap = Vec<Vec<String>>;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Coord {
    x: usize,
    y: usize,
    z: usize,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Verbs {
    Talk,
    Look,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Action {
    verb: Verbs,
    targets: Vec<String>,
    value: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Room {
    coord: Coord,
    description: String,
    verbs: Option<Vec<Action>>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Level {
    map: LevelMap,
    rooms: Vec<Room>,
}

fn main() {
    let path = PathBuf::from("data/levels/stone-end-market.yml");
    let yml_string = fs::read_to_string(path).expect("Could not load the level yml file.");
    let level: Level = serde_yaml::from_str(&yml_string).expect("Unable to parse the level");
    println!("Level {:#?}", level);
}
