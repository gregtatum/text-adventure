mod commands;
mod level;
mod print;
mod utils;

use crate::utils::parse_yml;
use level::{Coord, Direction, InventoryItem, ItemDatabase, Level, Room, RoomItem, Verb};
use print::{print_map_issue, print_room_description, print_text_file};
use serde::{Deserialize, Serialize};
use std::fs;
use std::{
    collections::HashMap, iter::Peekable, path::PathBuf, process, rc::Rc, str::SplitWhitespace,
};

fn get_prompt() -> String {
    rprompt::prompt_reply_stdout("» ").unwrap().to_lowercase()
}

#[derive(Debug, Clone)]
pub struct RoomMapInfo {
    north: Option<Coord>,
    east: Option<Coord>,
    south: Option<Coord>,
    west: Option<Coord>,
}

impl RoomMapInfo {
    fn from_direction(&self, direction: &Direction) -> &Option<Coord> {
        match direction {
            Direction::North => &self.north,
            Direction::East => &self.east,
            Direction::West => &self.west,
            Direction::South => &self.south,
        }
    }
}

enum RoomType {
    Normal,
}

fn parse_map(level: &Level) -> HashMap<Coord, RoomMapInfo> {
    // First build a map that can be queried by coordinates.
    let mut coord_map: HashMap<Coord, RoomType> = HashMap::new();
    for (z, map) in level.maps.iter().enumerate() {
        for (y, row) in map.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                match ch {
                    '.' => coord_map.insert(Coord { x, y, z }, RoomType::Normal),
                    '#' | '-' => None,
                    // This is a comment.
                    ' ' => break,
                    _ => {
                        eprintln!("Unknown character in a map.");
                        print_map_issue(&level, &Coord { x, y, z });
                        process::exit(1);
                    }
                };
            }
        }
    }

    let mut room_map: HashMap<Coord, RoomMapInfo> = HashMap::new();

    for (coord, _room_type) in coord_map.iter() {
        let north_coord = coord.apply(&Direction::North);
        let east_coord = coord.apply(&Direction::East);
        let south_coord = coord.apply(&Direction::South);
        let west_coord = coord.apply(&Direction::West);

        if let None = level.get_room(&coord) {
            eprintln!("Empty rooms were found in the map. Add the following:\n");

            for (coord, _) in coord_map.iter() {
                if let None = level.get_room(&coord) {
                    eprintln!("  - title: TODO",);
                    eprintln!("    coord: [{}, {}, {}]", coord.x, coord.y, coord.z);
                    eprintln!("    description: TODO",);
                }
            }

            eprintln!("");
            print_map_issue(&level, &coord);
            process::exit(1);
        };

        room_map.insert(
            coord.clone(),
            RoomMapInfo {
                north: match coord_map.get(&coord.apply(&Direction::North)) {
                    Some(RoomType::Normal) => Some(north_coord),
                    None => None,
                },
                east: match coord_map.get(&coord.apply(&Direction::East)) {
                    Some(RoomType::Normal) => Some(east_coord),
                    None => None,
                },
                south: match coord_map.get(&coord.apply(&Direction::South)) {
                    Some(RoomType::Normal) => Some(south_coord),
                    None => None,
                },
                west: match coord_map.get(&coord.apply(&Direction::West)) {
                    Some(RoomType::Normal) => Some(west_coord),
                    None => None,
                },
            },
        );
    }

    room_map
}

enum ParsedCommand {
    Look(Option<String>),
    Talk(Option<String>),
    Message(String),
    Inventory,
    Help(Option<String>),
    Move(Direction),
    Drop(String),
    Take(String),
    Quit,
    Debug,
    Restart,
}

#[derive(Serialize, Deserialize)]
struct Inventory {
    pub items: Vec<InventoryItem>,
}

impl From<Vec<InventoryItem>> for Inventory {
    fn from(items: Vec<InventoryItem>) -> Inventory {
        Inventory { items }
    }
}

impl Inventory {
    fn add_item(&mut self, new_item: InventoryItem) {
        match self.items.iter_mut().find(|item| item.id == new_item.id) {
            Some(item) => item.quantity += new_item.quantity,
            None => self.items.push(new_item),
        }
    }
}

enum DropResult {
    Item(InventoryItem),
    Sticky,
    None,
}

impl Inventory {
    pub fn drop_item(&mut self, name: &str) -> DropResult {
        // Find the item if it exists.
        let tuple = self
            .items
            .iter()
            .enumerate()
            .find(|(_, item)| item.name.to_lowercase() == name || item.targets.contains(name));

        match tuple {
            Some((index, item)) => {
                if item.sticky {
                    return DropResult::Sticky;
                }

                let removed_item = item.clone();

                // Remove the item.
                self.items = self
                    .items
                    .drain(..)
                    .enumerate()
                    .filter(|(i, _)| *i != index)
                    .map(|(_, item)| item)
                    .collect();

                DropResult::Item(removed_item)
            }
            None => DropResult::None,
        }
    }
}

fn parse_command_target(
    command: &str,
    words: &mut Peekable<SplitWhitespace>,
) -> Result<Option<String>, String> {
    let word = match words.next() {
        Some(word) => word,
        None => return Ok(None),
    };

    let mut target: String = match word {
        "at" | "to" | "in" | "up" => {
            if words.peek().is_none() {
                return Err(format!("{} {}... what?", command, word));
            }
            String::new()
        }
        _ => word.to_string(),
    };

    while let Some(word) = words.next() {
        target.push_str(word);
        if words.peek().is_some() {
            target.push(' ');
        }
    }

    Ok(Some(target))
}

fn parse_command(input: String) -> Result<ParsedCommand, String> {
    let mut words = input.split_whitespace().peekable();
    let command = match words.next() {
        Some(command) => command,
        None => {
            // No input was given.
            return Ok(ParsedCommand::Look(None));
        }
    };

    match command {
        "look" | "l" => Ok(ParsedCommand::Look(parse_command_target(
            &command, &mut words,
        )?)),
        "talk" | "t" => Ok(ParsedCommand::Talk(parse_command_target(
            &command, &mut words,
        )?)),
        "north" | "n" => Ok(ParsedCommand::Move(Direction::North)),
        "east" | "e" => Ok(ParsedCommand::Move(Direction::East)),
        "south" | "s" => Ok(ParsedCommand::Move(Direction::South)),
        "west" | "w" => Ok(ParsedCommand::Move(Direction::West)),
        "inventory" | "inv" | "i" | "items" => Ok(ParsedCommand::Inventory),
        "go" => match parse_command_target(&command, &mut words)? {
            Some(ref s) => match s.as_str() {
                "north" => Ok(ParsedCommand::Move(Direction::North)),
                "east" => Ok(ParsedCommand::Move(Direction::East)),
                "south" => Ok(ParsedCommand::Move(Direction::South)),
                "west" => Ok(ParsedCommand::Move(Direction::West)),
                _ => Err(format!("You don't know how to go {:?}", s)),
            },
            None => Ok(ParsedCommand::Message("Where do you want to go?".into())),
        },
        "" => Ok(ParsedCommand::Message("".into())),
        "help" | "h" => Ok(ParsedCommand::Help(parse_command_target(
            &command, &mut words,
        )?)),
        "debug" => Ok(ParsedCommand::Debug),
        "drop" => match parse_command_target(&command, &mut words)? {
            Some(target) => Ok(ParsedCommand::Drop(target)),
            None => Ok(ParsedCommand::Message("You stop drop and roll.".into())),
        },
        "pick" | "pickup" | "take" | "grab" => match parse_command_target(&command, &mut words)? {
            Some(target) => Ok(ParsedCommand::Take(target)),
            None => match command {
                "pick" => Err(format!("You pick your nose. Gross.")),
                _ => Err(format!(
                    "This relationship is on the rocks, all you do is take take take."
                )),
            },
        },
        "quit" | "q" | "exit" => Ok(ParsedCommand::Quit),
        "restart" => Ok(ParsedCommand::Restart),
        _ => Ok(ParsedCommand::Message(format!(
            "You don't know how to {:?}. Type \"help\" for help.",
            command
        ))),
    }
}

struct Game<'a> {
    level: Level,
    room: Rc<Room>,
    item_db: &'a ItemDatabase,
    save_state: SaveState,
    lookup_room_info: HashMap<Coord, RoomMapInfo>,
    room_info: RoomMapInfo,
}

impl<'a> Game<'a> {
    fn new(item_db: &ItemDatabase) -> Game {
        let level: Level = parse_yml(&"data/levels/stone-end-market.yml".into());
        let save_state = {
            let path = PathBuf::from("data/save-state.yml");
            if path.exists() {
                parse_yml(&"data/save-state.yml".into())
            } else {
                SaveState::initialize(&item_db, &level)
            }
        };
        let lookup_room_info = parse_map(&level);
        let room = (*level
            .get_room(&save_state.coord)
            .expect("Unable to find the entry room."))
        .clone();

        let room_info = (*lookup_room_info.get(&save_state.coord).unwrap()).clone();

        Game {
            level,
            room,
            item_db,
            save_state,
            lookup_room_info,
            room_info,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SaveState {
    /// The current room coordinate.
    coord: Coord,
    /// Turn on debug logging.
    debug: bool,
    /// The player's inventory.
    inventory: Inventory,
    room_inventories: HashMap<Coord, RoomInventory>,
}

impl SaveState {
    fn room_inventory_mut(&mut self) -> &mut RoomInventory {
        self.room_inventories
            .get_mut(&self.coord)
            .expect("Could not find a room inventory.")
    }
}

#[derive(Serialize, Deserialize)]
struct RoomInventory {
    inventory: Vec<(RoomItem, InventoryItem)>,
}

impl From<Vec<(RoomItem, InventoryItem)>> for RoomInventory {
    fn from(inventory: Vec<(RoomItem, InventoryItem)>) -> RoomInventory {
        RoomInventory { inventory }
    }
}

impl RoomInventory {
    pub fn take_item(&mut self, id: &str) -> Option<(RoomItem, InventoryItem)> {
        let mut inventory = Vec::new();
        let mut found_item = None;
        for item in self.inventory.drain(..) {
            let (ref room_item, ref inventory_item) = item;
            if found_item.is_some() {
                inventory.push(item);
            } else if room_item.targets.contains(id) {
                found_item = Some(item);
            } else if inventory_item.targets.contains(id) {
                found_item = Some(item);
            } else {
                inventory.push(item);
            }
        }
        self.inventory = inventory;
        found_item
    }

    fn add_item(&mut self, inventory_item: InventoryItem) {
        self.inventory
            .push((RoomItem::from(&inventory_item), inventory_item));
    }

    pub fn item_names_iter<'a>(&'a self) -> impl Iterator<Item = &'a str> {
        self.inventory
            .iter()
            .map(|(room_item, inv_item)| match room_item.name {
                Some(ref name) => name.as_str(),
                None => &inv_item.name,
            })
    }
}

impl SaveState {
    fn initialize(item_db: &ItemDatabase, level: &Level) -> SaveState {
        SaveState {
            coord: level.entry,
            debug: false,
            inventory: Inventory::from(vec![
                //
                item_db.get("sword").clone(),
                item_db.get("gold").clone(),
            ]),
            room_inventories: {
                let mut room_inventories = HashMap::new();
                for room in level.rooms.iter() {
                    let mut room_inventory: Vec<(RoomItem, InventoryItem)> = Vec::new();
                    // Fill the room item in with the actual item from the item db.
                    for room_item in room.items.iter() {
                        let room_item = room_item.clone();
                        let mut inventory_item = item_db.get(&room_item.id).clone();
                        inventory_item.quantity = room_item.quantity;
                        room_inventory.push((room_item, inventory_item));
                    }
                    room_inventories.insert(room.coord, RoomInventory::from(room_inventory));
                }
                room_inventories
            },
        }
    }
}

enum GameLoopResponse {
    Restart,
    Quit,
}

fn main() {
    loop {
        match game_loop() {
            GameLoopResponse::Restart => {
                let save_file = PathBuf::from("data/save-state.yml");
                if save_file.exists() {
                    fs::remove_file(PathBuf::from("data/save-state.yml"))
                        .expect("Unable to remove the save file.");
                }
            }
            GameLoopResponse::Quit => {
                println!("Thanks for playing!");
                return;
            }
        };
    }
}

fn game_loop() -> GameLoopResponse {
    let item_db = ItemDatabase::new();
    let mut game = Game::new(&item_db);

    print_text_file("data/intro.txt");
    print_room_description(&game.room, &game.save_state, &game.room_info);

    loop {
        let string = get_prompt();
        // Add a newline after the prompt.
        println!("");
        match parse_command(string).unwrap_or_else(|message| ParsedCommand::Message(message)) {
            ParsedCommand::Look(Some(target)) => {
                look_command(&game, &target);
            }
            ParsedCommand::Look(None) => {
                print_room_description(&game.room, &game.save_state, &game.room_info)
            }
            ParsedCommand::Help(Some(target)) => {
                help_target_command(&game, &target);
            }
            ParsedCommand::Help(None) => print_text_file("data/help.txt"),
            ParsedCommand::Move(direction) => {
                let next_coord: Option<Coord> = (game.room_info.from_direction(&direction)).clone();

                match next_coord {
                    Some(next_coord) => {
                        game.save_state.coord = next_coord.clone();
                        game.room_info =
                            (game.lookup_room_info.get(&game.save_state.coord).unwrap()).clone();

                        game.room = game
                            .level
                            .get_room(&next_coord)
                            .expect("Expected to find a room.")
                            .clone();
                        print_room_description(&game.room, &game.save_state, &game.room_info);
                    }
                    None => {
                        eprintln!("You cannot move {}.", direction.lowercase_string());
                    }
                };
            }
            ParsedCommand::Debug => {
                game.save_state.debug = !game.save_state.debug;
                if game.save_state.debug {
                    println!("Debug mode activated.");
                } else {
                    println!("Debug mode de-activated.");
                }
            }
            ParsedCommand::Drop(target) => match game.save_state.inventory.drop_item(&target) {
                DropResult::Item(item) => {
                    println!("You dropped the {}.", item.name);
                    game.save_state.room_inventory_mut().add_item(item);
                }
                DropResult::Sticky => {
                    println!("The {} appear(s) to be sticking to your hand.", target)
                }
                DropResult::None => {
                    println!("It does not look like you have a {}.", target);
                }
            },
            ParsedCommand::Take(target) => {
                match game.save_state.room_inventory_mut().take_item(&target) {
                    Some((room_item, inventory_item)) => {
                        game.save_state.inventory.add_item(inventory_item);
                        match room_item.pickup {
                            Some(pickup) => {
                                println!("{}", pickup)
                            }
                            None => {
                                println!("You place the {} in your inventory.", target)
                            }
                        }
                    }
                    None => {
                        println!("You couldn't find a {} to take.", target);
                    }
                }
            }
            ParsedCommand::Quit => {
                let path = PathBuf::from("data/save-state.yml");
                let yml = serde_yaml::to_string(&game.save_state)
                    .expect("Unable to serialize the game state.");
                fs::write(path, yml).expect("Unable to save the game state.");

                return GameLoopResponse::Quit;
            }
            ParsedCommand::Talk(Some(target)) => {
                match game.room.find_action(Verb::Talk, &target, &game.level) {
                    Some(action) => {
                        println!("{}", action.value);
                    }
                    None => {
                        println!("You can't talk to {:?}", target);
                    }
                }
            }
            ParsedCommand::Talk(None) => {
                println!("You talk outloud for a bit and feel much better, thank you.")
            }
            ParsedCommand::Inventory => {
                print_box("Your inventory:");
                if game.save_state.inventory.items.is_empty() {
                    println!("    (empty)")
                }
                for item in game.save_state.inventory.items.iter() {
                    match item.max_quantity {
                        Some(_) => {
                            println!("  ‣ {} ({})", item.name, item.quantity);
                        }
                        None => {
                            println!("  ‣ {}", item.name);
                        }
                    }
                }
                println!("");
            }
            ParsedCommand::Message(message) => println!("{}", message),
            ParsedCommand::Restart => {
                if prompt_yes_no("Are you sure you want to erase your game and restart?") {
                    return GameLoopResponse::Restart;
                } else {
                    println!("Let's keep playing!");
                }
            }
        }
    }
}

fn print_box(text: &str) {
    let len = text.len() + 2;
    print!("╔");
    for _ in 0..len {
        print!("═");
    }
    print!("╗\n");

    println!("║ {} ║", text);

    print!("╚");
    for _ in 0..len {
        print!("═");
    }
    print!("╝\n");
}

fn prompt_yes_no(message: &str) -> bool {
    loop {
        println!("{} (yes, no)", message);
        let response = get_prompt();
        match response.as_str() {
            "yes" | "y" => {
                return true;
            }
            "no" | "n" => {
                return false;
            }
            _ => {
                println!("What was that?");
            }
        }
    }
}

fn look_command(game: &Game, target: &String) {
    // Look at something in the room through an action?
    if let Some(action) = game.room.find_action(Verb::Look, &target, &game.level) {
        println!("{}\n", action.value);
        return;
    }

    // Look at an npc?
    if let Some(npc) = game.room.get_npc(&game.level, &target) {
        println!("{}\n", npc.description);
        for (item, cost) in npc.items_iter(&game.item_db) {
            println!("  ‣ {} ({} gp)", item.name, cost);
        }
        println!("");
        return;
    }

    // Look at an npc's item?
    for npc in game.room.npcs_iter(&game.level) {
        for sale_item in npc.items.iter() {
            if *target == sale_item.id {
                let item = game.item_db.get(target);
                println!("{}\n", item.description);
                return;
            }
        }
    }

    // Look at your own items?
    for inv_item in game.save_state.inventory.items.iter() {
        if *target == inv_item.id {
            let item = game.item_db.get(target);
            println!("{}\n", item.description);
            return;
        }
    }

    println!("You don't see a {}.\n", target);
}

fn help_target_command(game: &Game, target: &String) {
    // Help something in the room through an action?
    if let Some(action) = game.room.find_action(Verb::Help, &target, &game.level) {
        println!("{}\n", action.value);
        return;
    }

    println!("You can't help {}.\n", target);
}
