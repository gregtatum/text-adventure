use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, fs, path::PathBuf, process};

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
    fn print_description(&self, room_map_info: &RoomMapInfo) {
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
    Look(Option<CommandTarget>),
    Talk(Option<CommandTarget>),
    Message(String),
    Help,
    Unknown,
    Move(Direction),
    Quit,
}

struct CommandTarget(String);

enum CommandParseError {
    PrepositionError(String),
}

fn parse_command_target(parts: &Vec<&str>) -> Result<Option<CommandTarget>, CommandParseError> {
    if parts.len() == 1 || parts.len() == 0 {
        return Ok(None);
    }

    let word = parts.get(1).unwrap();
    match word {
        &"at" | &"to" => {
            if parts.len() == 2 {
                Err(CommandParseError::PrepositionError(word.to_string()))
            } else {
                Ok(Some(CommandTarget(parts[2..].join(" "))))
            }
        }
        _ => Ok(Some(CommandTarget(parts[1..].join(" ")))),
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
        "help" => ParsedCommand::Help,
        "quit" | "q" | "exit" => ParsedCommand::Quit,
        _ => ParsedCommand::Unknown,
    }
}

fn main() {
    let path = PathBuf::from("data/levels/stone-end-market.yml");
    let yml_string = fs::read_to_string(path).expect("Could not load the level yml file.");
    let level: Level = serde_yaml::from_str(&yml_string).expect("Unable to parse the level");

    let mut coord = level.entry;
    let mut room = level
        .get_room(&level.entry)
        .expect("Unable to find the entry room.");

    let lookup_room_info = parse_map(&level);
    let mut room_info = lookup_room_info.get(&coord).unwrap();

    print_text_file("data/intro.txt");
    room.print_description(&room_info);

    loop {
        let string = get_prompt();
        // Add a newline after the prompt.
        println!("");
        match parse_command(string) {
            ParsedCommand::Look(Some(target)) => match room.find_action(Verb::Look, &target.0) {
                Some(action) => {
                    println!("{}", action.value);
                }
                None => {
                    println!("You don't see {:?}", target.0);
                }
            },
            ParsedCommand::Look(None) => room.print_description(&room_info),
            ParsedCommand::Help => print_text_file("data/help.txt"),
            ParsedCommand::Unknown => println!("Unknown command. Type \"help\" for help."),
            ParsedCommand::Move(direction) => {
                match room_info.from_direction(&direction) {
                    Some(next_coord) => {
                        coord = next_coord.clone();
                        room_info = lookup_room_info.get(&coord).unwrap();
                        room = level
                            .get_room(&next_coord)
                            .expect("Expected to find a room.");
                        room.print_description(&room_info);
                    }
                    None => {
                        eprintln!("You cannot move {}.", direction.lowercase_string());
                    }
                };
            }
            ParsedCommand::Quit => {
                println!("Thanks for playing!");
                return;
            }
            ParsedCommand::Talk(Some(target)) => match room.find_action(Verb::Talk, &target.0) {
                Some(action) => {
                    println!("{}", action.value);
                }
                None => {
                    println!("You can't talk to {:?}", target.0);
                }
            },
            ParsedCommand::Talk(None) => {
                println!("You talk outloud for a bit and feel much better, thank you.")
            }
            ParsedCommand::Message(message) => println!("{}", message),
        }
    }
}
