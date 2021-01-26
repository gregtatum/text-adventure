use std::{fs, path::PathBuf, process};

use serde::de::DeserializeOwned;

pub fn parse_yml<T>(path: &PathBuf) -> T
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
                        for _ in 0..location.column() {
                            print!(" ");
                        }
                        println!("^ {}", err);
                        break;
                    }
                }
            }
            process::exit(1);
        }
    }
}
