mod level;
mod print;
mod utils;

use crate::utils::parse_yml;
use level::{Coord, Direction, InventoryItem, ItemDatabase, Level, Room, RoomItem, Verb};
use print::{print_map_issue, print_room_description, print_text_file};
use serde::{Deserialize, Serialize};
use std::{
    cell::{RefCell, RefMut},
    collections::HashMap,
    fs,
    io::{Stdout, Write},
    iter::Peekable,
    path::PathBuf,
    process,
    rc::Rc,
    str::SplitWhitespace,
};

pub trait Environment: Write {
    fn get_prompt(&mut self) -> String;
}

struct Terminal {
    stdout: Stdout,
}

impl Terminal {
    fn new() -> Terminal {
        Terminal {
            stdout: std::io::stdout(),
        }
    }
}

impl Environment for Terminal {
    fn get_prompt(&mut self) -> String {
        rprompt::prompt_reply_stdout("» ")
            .expect("Unable to get prompt")
            .to_lowercase()
    }
}

impl Write for Terminal {
    fn write(&mut self, buffer: &[u8]) -> Result<usize, std::io::Error> {
        self.stdout.write(buffer)
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.stdout.flush()
    }
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
    Custom(String, Option<String>),
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
        _ => Ok(ParsedCommand::Custom(
            command.to_string(),
            parse_command_target(&command, &mut words)?,
        )),
    }
}

pub struct Game<'a, T: Environment> {
    level: Level,
    room: Rc<Room>,
    item_db: &'a ItemDatabase,
    save_state: SaveState,
    lookup_room_info: HashMap<Coord, RoomMapInfo>,
    room_info: RoomMapInfo,
    environment: RefCell<T>,
}

impl<'a, T> Game<'a, T>
where
    T: Environment,
{
    fn new(
        item_db: &'a ItemDatabase,
        environment: T,
        level: Level,
        save_state: Option<SaveState>,
    ) -> Game<'a, T> {
        let save_state = save_state.unwrap_or_else(|| SaveState::initialize(&item_db, &level));
        let lookup_room_info = parse_map(&level);
        let room = (*level
            .get_room(&save_state.coord)
            .expect("Unable to find the entry room."))
        .clone();

        let room_info = (*lookup_room_info
            .get(&save_state.coord)
            .expect("Could not find coordinate."))
        .clone();

        Game {
            level,
            room,
            item_db,
            save_state,
            lookup_room_info,
            room_info,
            environment: RefCell::new(environment),
        }
    }

    fn output(&self) -> RefMut<T> {
        self.environment.borrow_mut()
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
    Quit(SaveState),
}

fn main() {
    let item_db = ItemDatabase::new();
    loop {
        let level: Level = parse_yml(&"data/levels/stone-end-market.yml".into());
        let save_state = {
            let path = PathBuf::from("data/save-state.yml");
            if path.exists() {
                Some(parse_yml(&"data/save-state.yml".into()))
            } else {
                None
            }
        };

        match game_loop(&item_db, level, save_state, Terminal::new()) {
            GameLoopResponse::Restart => {
                let save_file = PathBuf::from("data/save-state.yml");
                if save_file.exists() {
                    fs::remove_file(PathBuf::from("data/save-state.yml"))
                        .expect("Unable to remove the save file.");
                }
            }
            GameLoopResponse::Quit(save_state) => {
                let path = PathBuf::from("data/save-state.yml");
                let yml = serde_yaml::to_string(&save_state)
                    .expect("Unable to serialize the game state.");
                fs::write(path, yml).expect("Unable to save the game state.");

                println!("Thanks for playing!");
                return;
            }
        };
    }
}

fn game_loop<'a, T: Environment>(
    item_db: &'a ItemDatabase,
    level: Level,
    save_state: Option<SaveState>,
    environment: T,
) -> GameLoopResponse {
    let mut game = Game::new(&item_db, environment, level, save_state);

    writeln!(game.output(), "{}", game.level.intro).unwrap();
    print_room_description(&game);

    loop {
        let string = game.environment.borrow_mut().get_prompt();
        // Add a newline after the prompt.
        outputln!(game, "");
        match parse_command(string).unwrap_or_else(|message| ParsedCommand::Message(message)) {
            ParsedCommand::Look(Some(target)) => {
                look_command(&mut game, &target);
            }
            ParsedCommand::Look(None) => print_room_description(&game),
            ParsedCommand::Help(Some(target)) => {
                help_target_command(&game, &target);
            }
            ParsedCommand::Help(None) => print_text_file(&game, "data/help.txt"),
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
                        print_room_description(&game);
                    }
                    None => {
                        eprintln!("You cannot move {}.", direction.lowercase_string());
                    }
                };
            }
            ParsedCommand::Debug => {
                game.save_state.debug = !game.save_state.debug;
                if game.save_state.debug {
                    outputln!(game, "Debug mode activated.");
                } else {
                    outputln!(game, "Debug mode de-activated.");
                }
            }
            ParsedCommand::Drop(target) => match game.save_state.inventory.drop_item(&target) {
                DropResult::Item(item) => {
                    outputln!(game, "You dropped the {}.", item.name);
                    game.save_state.room_inventory_mut().add_item(item);
                }
                DropResult::Sticky => {
                    outputln!(
                        game,
                        "The {} appear(s) to be sticking to your hand.",
                        target
                    )
                }
                DropResult::None => {
                    outputln!(game, "It does not look like you have a {}.", target);
                }
            },
            ParsedCommand::Take(target) => {
                match game.save_state.room_inventory_mut().take_item(&target) {
                    Some((room_item, inventory_item)) => {
                        game.save_state.inventory.add_item(inventory_item);
                        match room_item.pickup {
                            Some(pickup) => {
                                outputln!(game, "{}", pickup)
                            }
                            None => {
                                outputln!(game, "You place the {} in your inventory.", target)
                            }
                        }
                    }
                    None => {
                        outputln!(game, "You couldn't find a {} to take.", target);
                    }
                }
            }
            ParsedCommand::Quit => {
                return GameLoopResponse::Quit(game.save_state);
            }
            ParsedCommand::Talk(Some(target)) => {
                match game
                    .room
                    .find_action(Verb::Talk, &target, &game.level, None)
                {
                    Some(action) => {
                        outputln!(game, "{}", action.value);
                    }
                    None => {
                        outputln!(game, "You can't talk to {:?}", target);
                    }
                }
            }
            ParsedCommand::Talk(None) => {
                outputln!(
                    game,
                    "You talk outloud for a bit and feel much better, thank you."
                )
            }
            ParsedCommand::Inventory => {
                print_box(&game, "Your inventory:");
                if game.save_state.inventory.items.is_empty() {
                    outputln!(game, "    (empty)")
                }
                for item in game.save_state.inventory.items.iter() {
                    match item.max_quantity {
                        Some(_) => {
                            outputln!(game, "  ‣ {} ({})", item.name, item.quantity);
                        }
                        None => {
                            outputln!(game, "  ‣ {}", item.name);
                        }
                    }
                }
                outputln!(game, "");
            }
            ParsedCommand::Message(message) => outputln!(game, "{}", message),
            ParsedCommand::Restart => {
                if prompt_yes_no(
                    &mut game,
                    "Are you sure you want to erase your game and restart?",
                ) {
                    return GameLoopResponse::Restart;
                } else {
                    outputln!(game, "Let's keep playing!");
                }
            }
            ParsedCommand::Custom(command, target) => match target {
                Some(target) => {
                    match game
                        .room
                        .find_action(Verb::Custom, &target, &game.level, Some(&command))
                    {
                        Some(action) => outputln!(game, "{}", action.value),
                        None => outputln!(
                            game,
                            "You don't know how to {:?}. Type \"help\" for help.",
                            command
                        ),
                    }
                }
                None => outputln!(
                    game,
                    "You don't know how to {:?}. Type \"help\" for help.",
                    command
                ),
            },
        }
    }
}

fn print_box<T: Environment>(game: &Game<T>, text: &str) {
    let len = text.len() + 2;
    output!(game, "╔");
    for _ in 0..len {
        output!(game, "═");
    }
    output!(game, "╗\n");

    outputln!(game, "║ {} ║", text);

    output!(game, "╚");
    for _ in 0..len {
        output!(game, "═");
    }
    output!(game, "╝\n");
}

fn prompt_yes_no<T: Environment>(game: &mut Game<T>, message: &str) -> bool {
    loop {
        outputln!(game, "{} (yes, no)", message);
        let response = game.environment.borrow_mut().get_prompt();
        match response.as_str() {
            "yes" | "y" => {
                return true;
            }
            "no" | "n" => {
                return false;
            }
            _ => {
                outputln!(game, "What was that?");
            }
        }
    }
}

fn look_command<T: Environment>(game: &Game<T>, target: &String) {
    // Look at something in the room through an action?
    if let Some(action) = game
        .room
        .find_action(Verb::Look, &target, &game.level, None)
    {
        writeln!(game.output(), "{}\n", action.value).unwrap();
        return;
    }

    // Look at an npc?
    if let Some(npc) = game.room.get_npc(&game.level, &target) {
        outputln!(game, "{}\n", npc.description);
        for (item, cost) in npc.items_iter(&game.item_db) {
            outputln!(game, "  ‣ {} ({} gp)", item.name, cost);
        }
        outputln!(game, "");
        return;
    }

    // Look at an npc's item?
    for npc in game.room.npcs_iter(&game.level) {
        for sale_item in npc.items.iter() {
            if *target == sale_item.id {
                let item = game.item_db.get(target);
                outputln!(game, "{}\n", item.description);
                return;
            }
        }
    }

    // Look at your own items?
    for inv_item in game.save_state.inventory.items.iter() {
        if *target == inv_item.id {
            let item = game.item_db.get(target);
            outputln!(game, "{}\n", item.description);
            return;
        }
    }

    outputln!(game, "You don't see a {}.\n", target);
}

fn help_target_command<T: Environment>(game: &Game<T>, target: &String) {
    // Help something in the room through an action?
    if let Some(action) = game
        .room
        .find_action(Verb::Help, &target, &game.level, None)
    {
        outputln!(game, "{}\n", action.value);
        return;
    }

    outputln!(game, "You can't help {}.\n", target);
}

#[macro_export]
macro_rules! output {
    ( $game:ident, $( $x:expr ),* ) => {
        write!($game.output(), $($x, )*).unwrap();
    };
}

#[macro_export]
macro_rules! outputln {
    ( $game:ident, $( $x:expr ),* ) => {
        writeln!($game.output(), $($x, )*).unwrap();
    };
}

#[cfg(test)]
mod test {
    use insta::assert_yaml_snapshot;

    use super::*;

    struct CommandRunner {
        commands: Vec<&'static str>,
        output: Vec<u8>,
    }

    impl CommandRunner {
        fn new(mut commands: Vec<&'static str>) -> CommandRunner {
            commands.push("quit");
            commands.reverse();
            CommandRunner {
                commands,
                output: Vec::new(),
            }
        }

        fn get_last_output<'a>(self) -> Vec<String> {
            std::str::from_utf8(&self.output)
                .unwrap()
                .lines()
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
    }

    impl Write for CommandRunner {
        fn write(&mut self, buffer: &[u8]) -> Result<usize, std::io::Error> {
            for value in buffer.iter() {
                self.output.push(*value);
            }
            Ok(buffer.len())
        }

        fn flush(&mut self) -> Result<(), std::io::Error> {
            Ok(())
        }
    }

    impl Environment for &mut CommandRunner {
        fn get_prompt(&mut self) -> String {
            if self.commands.len() > 1 {
                // Retain the last output.
                self.output.clear();
            }
            self.commands
                .pop()
                .expect("Expected to find a string in the CommandRunner.")
                .to_string()
        }
    }

    fn run_game(commands: Vec<&'static str>) -> Vec<String> {
        let item_db = ItemDatabase::new();
        let mut command_runner = CommandRunner::new(commands);
        let level: Level = parse_yml(&"data/levels/test-level.yml".into());

        match game_loop(&item_db, level, None, &mut command_runner) {
            GameLoopResponse::Quit(_) => {}
            GameLoopResponse::Restart => panic!("Unexpected restart."),
        };

        command_runner.get_last_output()
    }

    #[test]
    fn test_intro() {
        assert_yaml_snapshot!(run_game(vec![]), @r###"
        ---
        - ╔═══════╗
        - ║ Intro ║
        - ╚═══════╝
        - Stone End Docks
        - "    You are standing at the Stone End Docks. "
        - "Exits: n _ _ _"
        "###);
    }

    #[test]
    fn test_look() {
        assert_yaml_snapshot!(run_game(vec!["look"]), @r###"
        ---
        - Stone End Docks
        - "    You are standing at the Stone End Docks. "
        - "Exits: n _ _ _"
        "###);
    }

    #[test]
    fn test_inventory() {
        assert_yaml_snapshot!(run_game(vec!["inventory"]), @r###"
        ---
        - ╔═════════════════╗
        - "║ Your inventory: ║"
        - ╚═════════════════╝
        - "  ‣ sword"
        - "  ‣ gold (17)"
        "###);
    }

    #[test]
    fn test_look_inventory() {
        assert_yaml_snapshot!(run_game(vec!["look sword"]), @r###"
        ---
        - "A fairly basic looking sword, with some signs of wear. It is well oiled and"
        - "sharpened, ready to use."
        "###);
    }

    #[test]
    fn test_look_room_verb() {
        assert_yaml_snapshot!(run_game(vec!["look post"]), @r###"
        ---
        - "The guard post is well worn. It looks old, but sturdy."
        "###);
    }

    #[test]
    fn test_look_unknown() {
        assert_yaml_snapshot!(run_game(vec!["look unknown"]), @r###"
        ---
        - "You don't see a unknown."
        "###);
    }

    #[test]
    fn test_look_region() {
        assert_yaml_snapshot!(run_game(vec!["look keep"]), @r###"
        ---
        - Stone End Keep is a towering structure that dominates the market. It is guarded and shut. Over the din of the market you can barely hear the sounds of shouting.
        "###);
    }

    #[test]
    fn test_talk() {
        assert_yaml_snapshot!(run_game(vec!["talk guard"]), @r###"
        ---
        - "The guard glares at you and says, \"Welcome to Stone End, now scram!\""
        "###);
    }

    #[test]
    fn test_drop_item() {
        assert_yaml_snapshot!(run_game(vec!["inv"]), @r###"
        ---
        - ╔═════════════════╗
        - "║ Your inventory: ║"
        - ╚═════════════════╝
        - "  ‣ sword"
        - "  ‣ gold (17)"
        "###);

        assert_yaml_snapshot!(run_game(vec!["drop sword"]), @r###"
        ---
        - You dropped the sword.
        "###);

        assert_yaml_snapshot!(run_game(vec!["drop sword", "look"]), @r###"
        ---
        - Stone End Docks
        - "    You are standing at the Stone End Docks. "
        - sword
        - "Exits: n _ _ _"
        "###);

        assert_yaml_snapshot!(run_game(vec!["drop sword", "inv"]), @r###"
        ---
        - ╔═════════════════╗
        - "║ Your inventory: ║"
        - ╚═════════════════╝
        - "  ‣ gold (17)"
        "###);
    }
}
