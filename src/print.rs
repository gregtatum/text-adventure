use crate::{
    level::{Coord, Level, Room},
    RoomMapInfo, SaveState,
};
use std::{fs, path::PathBuf};

const LINE_WIDTH: usize = 90;
const INDENT: usize = 4;

pub fn print_exits(room_map_info: &RoomMapInfo) {
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

pub fn print_text_file(path_str: &str) {
    let path = PathBuf::from(path_str);
    let text = fs::read_to_string(path).expect("Could not find the intro.txt");
    println!("{}", text);
}

pub fn print_room_description(room: &Room, save_state: &SaveState, room_map_info: &RoomMapInfo) {
    println!("{}\n", room.title);

    let mut formatted_description = room.cached_formatted_description.borrow_mut();

    if formatted_description.len() == 0 {
        let paragraphs = room.description.split("\n\n");
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

    for name in save_state
        .room_inventories
        .get(&room.coord)
        .expect("room inventory")
        .item_names_iter()
    {
        println!("{}", name);
    }

    if !room.items.is_empty() {
        println!();
    }

    if save_state.debug {
        let Coord { x, y, z } = save_state.coord;
        println!("Coord: [{}, {}, {}]", x, y, z);
    }

    print_exits(room_map_info);
}

pub fn print_map_issue(level: &Level, coord: &Coord) {
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
