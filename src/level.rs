use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::utils::parse_yml;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub maps: LevelMap,
    pub rooms: Vec<Rc<Room>>,
    pub entry: Coord,
    pub npcs: HashMap<String, NPC>,
    pub regions: HashMap<String, Region>,
}

impl Level {
    pub fn get_room(&self, coord: &Coord) -> Option<&Rc<Room>> {
        self.rooms.iter().find(|room| room.coord == *coord)
    }
}

// The YML representation of a level. This gets parsed as a utility to verify
// the correct encoding of the level information.
// [
//     // Map 0
//     [
//         "---###---",
//         "---#.#---",
//         "---###---",
//     ],
//     // Map 1
//     [
//         "-#####---",
//         "-#...#---",
//         "-#####---",
//     ],
// ]
pub type LevelMap = Vec<Vec<String>>;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub title: String,
    pub coord: Coord,
    pub description: String,
    pub actions: Option<Vec<Action>>,
    #[serde(default)]
    pub cached_formatted_description: RefCell<String>,
    #[serde(default)]
    pub items: Vec<RoomItem>,
    #[serde(default)]
    pub npcs: Vec<String>,
    #[serde(default)]
    pub regions: Vec<String>,
}

impl Room {
    pub fn npcs_iter<'a>(&'a self, level: &'a Level) -> impl Iterator<Item = &'a NPC> {
        self.npcs
            .iter()
            .map(move |npc_id| match level.npcs.get(npc_id) {
                Some(npc) => npc,
                None => {
                    eprintln!("Unable to find an npc by the id {:?}", npc_id);
                    eprintln!("The available NPCs are:");
                    for key in level.npcs.keys() {
                        eprintln!("  {:?}", key);
                    }
                    panic!();
                }
            })
    }

    pub fn get_npc<'a>(&'a self, level: &'a Level, target: &String) -> Option<&'a NPC> {
        self.npcs_iter(&level)
            .find(|npc| npc.targets.contains(target))
    }

    pub fn find_action<'a>(
        &'a self,
        verb: Verb,
        target: &String,
        level: &'a Level,
    ) -> Option<&'a Action> {
        // Check this room for the action.
        if let Some(ref actions) = self.actions {
            if let Some(action) = actions
                .iter()
                .find(|action| action.verb == verb && action.targets.contains(target))
            {
                return Some(action);
            };
        }

        // The action could also be in a region. Find the actions for the regions.
        for region in self.regions.iter() {
            match level.regions.get(region) {
                Some(region) => {
                    if let Some(action) = region
                        .actions
                        .iter()
                        .find(|action| action.verb == verb && action.targets.contains(target))
                    {
                        return Some(action);
                    };
                }
                None => {
                    eprintln!("Unable to find a region from the id {:?}", region);
                    eprintln!("Available ids:");
                    for region_id in level.regions.keys() {
                        eprintln!("  {:?}", region_id);
                    }
                    panic!()
                }
            }
        }
        None
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct Coord {
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

impl Coord {
    pub fn apply(&self, direction: &Direction) -> Coord {
        match direction {
            Direction::North => Coord {
                x: self.x,
                y: self.y - 1,
                z: self.z,
            },
            Direction::East => Coord {
                x: self.x + 1,
                y: self.y,
                z: self.z,
            },
            Direction::West => Coord {
                x: self.x - 1,
                y: self.y,
                z: self.z,
            },
            Direction::South => Coord {
                x: self.x,
                y: self.y + 1,
                z: self.z,
            },
        }
    }
}

pub enum Direction {
    North,
    East,
    West,
    South,
}

impl Direction {
    pub fn lowercase_string(&self) -> &str {
        match self {
            Direction::North => "north",
            Direction::East => "east",
            Direction::West => "west",
            Direction::South => "south",
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct NPC {
    pub name: String,
    pub description: String,
    pub targets: Vec<String>,
    pub talk: String,
    pub items: Vec<SaleItem>,
}

impl NPC {
    pub fn items_iter<'a>(
        &'a self,
        item_db: &'a ItemDatabase,
    ) -> impl Iterator<Item = (&'a InventoryItem, usize)> {
        self.items
            .iter()
            .map(move |SaleItem { ref id, ref cost }| (item_db.get(&id), *cost))
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SaleItem {
    pub id: String,
    pub cost: usize,
}

pub struct ItemDatabase {
    items: Vec<InventoryItem>,
}

impl ItemDatabase {
    pub fn new() -> ItemDatabase {
        ItemDatabase {
            items: parse_yml(&"data/items.yml".into()),
        }
    }

    pub fn get(&self, id: &str) -> &InventoryItem {
        let item = self.items.iter().find(|item| item.id == id);
        match item {
            Some(item) => item,
            None => {
                panic!("Unable to find the item with the id {}", id);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: String,
    pub name: String,
    pub targets: HashSet<String>,
    #[serde(default)]
    pub sticky: bool,
    pub variant: ItemVariant,
    #[serde(default)]
    pub quantity: usize,
    #[serde(default)]
    pub max_quantity: Option<usize>,
    pub description: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub verb: Verb,
    pub targets: Vec<String>,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoomItem {
    pub id: String,
    pub quantity: usize,
    pub name: Option<String>,
    pub targets: HashSet<String>,
    pub pickup: Option<String>,
}

impl From<&InventoryItem> for RoomItem {
    fn from(inventor_item: &InventoryItem) -> RoomItem {
        RoomItem {
            id: inventor_item.id.clone(),
            quantity: inventor_item.quantity,
            name: None,
            targets: HashSet::new(),
            pickup: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Verb {
    Help,
    Look,
    Talk,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ItemVariant {
    Consumable,
    Weapon,
    Money,
}
