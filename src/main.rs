use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    process,
};

const LINE_WIDTH: usize = 90;
const INDENT: usize = 4;

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
type LevelMap = Vec<Vec<String>>;

///
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize, Eq, Hash)]
struct Coord {
    x: usize,
    y: usize,
    z: usize,
}

impl Coord {
    fn apply(&self, direction: &Direction) -> Coord {
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

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Verb {
    Talk,
    Look,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Action {
    verb: Verb,
    targets: Vec<String>,
    value: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Room {
    title: String,
    coord: Coord,
    description: String,
    actions: Option<Vec<Action>>,
    #[serde(default)]
    cached_formatted_description: RefCell<String>,
}

impl Room {
    fn print_description(&self, state: &GameState, room_map_info: &RoomMapInfo) {
        println!("{}\n", self.title);

        let mut formatted_description = self.cached_formatted_description.borrow_mut();

        if formatted_description.len() == 0 {
            let paragraphs = self.description.split("\n\n");
            let mut formatted_lines = Vec::new();
            for paragraph in paragraphs {
                let paragraph = paragraph.replace('\n', " ");
                let mut formatted_line = " ".repeat(INDENT);
                for word in paragraph.split(' ') {
                    let word = word.trim();
                    if word.is_empty() {
                        continue;
                    }
                    if formatted_line.len() + word.len() > LINE_WIDTH {
                        formatted_line.push('\n');
                        formatted_lines.push(formatted_line);
                        formatted_line = " ".repeat(INDENT);
                    }
                    formatted_line.push_str(word);
                    formatted_line.push(' ');
                }
                formatted_lines.push(formatted_line);
                formatted_lines.push(String::from("\n\n"));
            }
            *formatted_description = formatted_lines.join("");
        }
        println!("{}", formatted_description);
        if state.debug {
            let Coord { x, y, z } = state.coord;
            println!("Coord: [{}, {}, {}]", x, y, z);
        }
        print_exits(room_map_info);
    }

    fn find_action(&self, verb: Verb, target: &String) -> Option<&Action> {
        match self.actions {
            Some(ref actions) => actions
                .iter()
                .find(|action| action.verb == verb && action.targets.contains(target)),
            None => None,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Level {
    maps: LevelMap,
    rooms: Vec<Room>,
    entry: Coord,
}

impl Level {
    fn get_room(&self, coord: &Coord) -> Option<&Room> {
        self.rooms.iter().find(|room| room.coord == *coord)
    }
}

fn print_exits(room_map_info: &RoomMapInfo) {
    let mut exits = String::from("Exits:");

    let mut push_dir = |option, string| match option {
        Some(_) => exits.push_str(string),
        None => exits.push_str(" _"),
    };

    push_dir(room_map_info.north, " n");
    push_dir(room_map_info.east, " e");
    push_dir(room_map_info.south, " s");
    push_dir(room_map_info.west, " w");
    println!("{}", exits);
}

fn get_prompt() -> String {
    rprompt::prompt_reply_stdout("» ").unwrap().to_lowercase()
}

fn print_text_file(path_str: &str) {
    let path = PathBuf::from(path_str);
    let text = fs::read_to_string(path).expect("Could not find the intro.txt");
    println!("{}", text);
}

#[derive(Debug)]
struct RoomMapInfo {
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

fn print_map_issue(level: &Level, coord: &Coord) {
    let map = match level.maps.get(coord.z) {
        Some(map) => map,
        None => {
            eprintln!("No map was found at layer: {:?}", coord.z);
            return;
        }
    };

    for (y, row) in map.iter().enumerate() {
        println!("{}", row);
        if y == coord.y {
            let mut indent = String::from(" ");
            indent = indent.repeat(coord.x);
            indent.push('^');
            println!("{}", indent);
            break;
        }
    }
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
            eprintln!("No room info was found in the map. Add the following coordinate:\n");
            eprintln!("  - coord: [{}, {}, {}]", coord.x, coord.y, coord.z);
            eprintln!("    description: TODO\n",);

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

enum Direction {
    North,
    East,
    West,
    South,
}

impl Direction {
    fn lowercase_string(&self) -> &str {
        match self {
            Direction::North => "north",
            Direction::East => "east",
            Direction::West => "west",
            Direction::South => "south",
        }
    }
}

enum ParsedCommand {
    Look(Option<String>),
    Talk(Option<String>),
    Message(String),
    Inventory,
    Help,
    Move(Direction),
    Drop(String),
    Quit,
    Debug,
    Restart,
}

enum CommandParseError {
    PrepositionError(String),
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
            .find(|(_, item)| item.name.to_lowercase() == name || item.aka.contains(name));

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

fn parse_command_target(parts: &Vec<&str>) -> Result<Option<String>, CommandParseError> {
    if parts.len() == 1 || parts.len() == 0 {
        return Ok(None);
    }

    let word = parts.get(1).unwrap();
    match word {
        &"at" | &"to" | &"in" => {
            if parts.len() == 2 {
                Err(CommandParseError::PrepositionError(word.to_string()))
            } else {
                Ok(Some(parts[2..].join(" ")))
            }
        }
        _ => Ok(Some(parts[1..].join(" "))),
    }
}

fn parse_command(input: String) -> ParsedCommand {
    let parts: Vec<&str> = input.split(' ').collect();
    if parts.is_empty() {
        return ParsedCommand::Look(None);
    }
    let command = *parts.get(0).unwrap();
    let target = match parse_command_target(&parts) {
        Ok(target) => target,
        Err(CommandParseError::PrepositionError(preposition)) => {
            return ParsedCommand::Message(format!("{} {} what?", command, preposition));
        }
    };

    match command {
        "look" | "l" => ParsedCommand::Look(target),
        "talk" | "t" => ParsedCommand::Talk(target),
        "north" | "n" => ParsedCommand::Move(Direction::North),
        "east" | "e" => ParsedCommand::Move(Direction::East),
        "south" | "s" => ParsedCommand::Move(Direction::South),
        "west" | "w" => ParsedCommand::Move(Direction::West),
        "inventory" | "inv" => ParsedCommand::Inventory,
        "go" => match target {
            Some(ref s) => match s.as_str() {
                "north" => ParsedCommand::Move(Direction::North),
                "east" => ParsedCommand::Move(Direction::East),
                "south" => ParsedCommand::Move(Direction::South),
                "west" => ParsedCommand::Move(Direction::West),
                _ => ParsedCommand::Message(format!("You don't know how to go {:?}", s)),
            },
            None => ParsedCommand::Message("Where do you want to go?".into()),
        },
        "" => ParsedCommand::Message("".into()),
        "help" => ParsedCommand::Help,
        "debug" => ParsedCommand::Debug,
        "drop" => match target {
            Some(target) => ParsedCommand::Drop(target),
            None => ParsedCommand::Message("You stop drop and roll.".into()),
        },
        "quit" | "q" | "exit" => ParsedCommand::Quit,
        "restart" => ParsedCommand::Restart,
        _ => ParsedCommand::Message(format!(
            "You don't know how to {:?}. Type \"help\" for help.",
            command
        )),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum ItemVariant {
    Consumable,
    Weapon,
    Money,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct InventoryItem {
    id: String,
    name: String,
    aka: HashSet<String>,
    #[serde(default)]
    sticky: bool,
    variant: ItemVariant,
    #[serde(default)]
    quantity: usize,
    #[serde(default)]
    max_quantity: Option<usize>,
}

struct ItemDatabase {
    items: Vec<InventoryItem>,
}

fn parse_yml<T>(path: &PathBuf) -> T
where
    T: DeserializeOwned,
{
    let yml_string = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => panic!("Could not load {:?}", path),
    };

    match serde_yaml::from_str(&yml_string) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("========================================================");
            eprintln!("Unable to deserialize: {:?}", path);

            if let Some(location) = err.location() {
                eprintln!("========================================================");
                let backscroll = 10;
                let backscroll_index = location.line() - backscroll.min(location.line());
                for (line_index, line) in yml_string.lines().enumerate() {
                    if line_index > backscroll_index {
                        eprintln!("{}", line);
                    }
                    if line_index == location.line() - 1 {
                        for _ in 0..location.line() {
                            print!(" ");
                        }
                        println!("^ {}", err);
                        break;
                    }
                }
            }
            process::exit(1);

            // panic!("Unable to deserialize {:?}\n{:?}", path, err)
        }
    }
}

impl ItemDatabase {
    fn new() -> ItemDatabase {
        ItemDatabase {
            items: parse_yml(&"data/items.yml".into()),
        }
    }

    fn get(&self, id: &str) -> &InventoryItem {
        let item = self.items.iter().find(|item| item.id == id);
        match item {
            Some(item) => item,
            None => {
                panic!("Unable to find the item with the id {}", id);
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct GameState {
    coord: Coord,
    debug: bool,
    inventory: Inventory,
}

impl GameState {
    fn initialize(item_db: &ItemDatabase, level: &Level) -> GameState {
        GameState {
            coord: level.entry,
            debug: false,
            inventory: Inventory::from(vec![
                //
                item_db.get("sword").clone(),
                item_db.get("gold").clone(),
            ]),
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
                fs::remove_file(PathBuf::from("data/save-state.yml"))
                    .expect("Unable to remove the save file.");
            }
            GameLoopResponse::Quit => {
                println!("Thanks for playing!");
                return;
            }
        };
    }
}

fn game_loop() -> GameLoopResponse {
    let level: Level = {
        let yml_string = fs::read_to_string(PathBuf::from("data/levels/stone-end-market.yml"))
            .expect("Could not load the level yml file.");
        serde_yaml::from_str(&yml_string).expect("Unable to parse the level")
    };
    let item_db = ItemDatabase::new();
    let mut state: GameState = {
        let path = PathBuf::from("data/save-state.yml");
        if path.exists() {
            parse_yml(&"data/save-state.yml".into())
        } else {
            GameState::initialize(&item_db, &level)
        }
    };

    let mut room = level
        .get_room(&level.entry)
        .expect("Unable to find the entry room.");

    let lookup_room_info = parse_map(&level);
    let mut room_info = lookup_room_info.get(&state.coord).unwrap();

    print_text_file("data/intro.txt");
    room.print_description(&state, &room_info);

    loop {
        let string = get_prompt();
        // Add a newline after the prompt.
        println!("");
        match parse_command(string) {
            ParsedCommand::Look(Some(target)) => match room.find_action(Verb::Look, &target) {
                Some(action) => {
                    println!("{}", action.value);
                }
                None => {
                    println!("You don't see a {}.", target);
                }
            },
            ParsedCommand::Look(None) => room.print_description(&state, &room_info),
            ParsedCommand::Help => print_text_file("data/help.txt"),
            ParsedCommand::Move(direction) => {
                match room_info.from_direction(&direction) {
                    Some(next_coord) => {
                        state.coord = next_coord.clone();
                        room_info = lookup_room_info.get(&state.coord).unwrap();
                        room = level
                            .get_room(&next_coord)
                            .expect("Expected to find a room.");
                        room.print_description(&state, &room_info);
                    }
                    None => {
                        eprintln!("You cannot move {}.", direction.lowercase_string());
                    }
                };
            }
            ParsedCommand::Debug => {
                state.debug = !state.debug;
                if state.debug {
                    println!("Debug mode activated.");
                } else {
                    println!("Debug mode de-activated.");
                }
            }
            ParsedCommand::Drop(target) => match state.inventory.drop_item(&target) {
                DropResult::Item(item) => {
                    println!("You dropped the {}.", item.name);
                }
                DropResult::Sticky => {
                    println!("The {} appear(s) to be sticking to your hand.", target)
                }
                DropResult::None => {
                    println!("It does not look like you have a {}.", target);
                }
            },
            ParsedCommand::Quit => {
                let path = PathBuf::from("data/save-state.yml");
                let yml =
                    serde_yaml::to_string(&state).expect("Unable to serialize the game state.");
                fs::write(path, yml).expect("Unable to save the game state.");

                return GameLoopResponse::Quit;
            }
            ParsedCommand::Talk(Some(target)) => match room.find_action(Verb::Talk, &target) {
                Some(action) => {
                    println!("{}", action.value);
                }
                None => {
                    println!("You can't talk to {:?}", target);
                }
            },
            ParsedCommand::Talk(None) => {
                println!("You talk outloud for a bit and feel much better, thank you.")
            }
            ParsedCommand::Inventory => {
                print_box("Your inventory:");
                if state.inventory.items.is_empty() {
                    println!("    (empty)")
                }
                for item in state.inventory.items.iter() {
                    match item.max_quantity {
                        Some(_) => {
                            println!("  • {} ({})", item.name, item.quantity);
                        }
                        None => {
                            println!("  • {}", item.name);
                        }
                    }
                    println!("");
                }
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
