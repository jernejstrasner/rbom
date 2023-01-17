use rbom::*;
use std::env;
use std::collections::HashMap;
use binary_parser::Binary;
use log::{error, warn};
use env_logger;

#[derive(Debug, Clone)]
struct File {
    path: String,
    parent: u32,
    kind: u8,
    architecture: u16,
    mode: u16,
    user: u32,
    group: u32,
    modtime: u32,
    size: u32,
    checksum: u32,
}

impl Default for File {
    fn default() -> Self {
        File {
            path: String::new(),
            parent: 0,
            kind: 0,
            architecture: 0,
            mode: 0,
            user: 0,
            group: 0,
            modtime: 0,
            size: 0,
            checksum: 0,
        }
    }
}

impl From<&[u8]> for File {
    fn from(bytes: &[u8]) -> Self {
        let mut bin = Binary::new(bytes);
        let mut file = File::default();
        file.kind = bin.parse_u8().unwrap();
        bin.skip(1);
        file.architecture = bin.parse_u16_be().unwrap();
        file.mode = bin.parse_u16_be().unwrap();
        file.user = bin.parse_u32_be().unwrap();
        file.group = bin.parse_u32_be().unwrap();
        file.modtime = bin.parse_u32_be().unwrap();
        file.size = bin.parse_u32_be().unwrap();
        bin.skip(1);
        file.checksum = bin.parse_u32_be().unwrap();
        file
    }
}

pub fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: lsbom <bom_file>");
        return;
    }

    let bom = Bom::with_file(&args[1]);
    
    // Extract the file infos
    let result = bom.reduce_tree_for_variable("Paths", HashMap::new(), |mut initial, key, val| {
        let id = u32::from_be_bytes(val[0..4].try_into().unwrap());
        let index = u32::from_be_bytes(val[4..8].try_into().unwrap());
        let parent = u32::from_be_bytes(key[0..4].try_into().unwrap());
        let path = std::str::from_utf8(&key[4..]).unwrap().trim_end_matches('\0');
        if let Some(file_info_ptr) = bom.pointer(index) {
            let file_info_buf = &bom.buffer[file_info_ptr.address as usize..file_info_ptr.address as usize + file_info_ptr.length as usize];
            let mut file_info = File::from(file_info_buf);
            file_info.parent = parent;
            file_info.path = path.to_string();
            initial.insert(id, file_info);
        } else {
            warn!("Could not find file info for path {:} with id {:}", path, id)
        }
        initial
    });

    if let Result::Err(e) = result {
        error!("{}", e);
        return;
    }
    let paths = result.unwrap();

    // Build the full paths
    let mut res = Vec::new();
    for (_, path) in &paths {
        let mut path_components = Vec::new();
        path_components.push(path.path.clone());
        let mut parent = paths.get(&path.parent);
        while let Some(p) = parent {
            path_components.push(p.path.clone());
            parent = paths.get(&p.parent);
        }
        path_components.reverse();
        let full_path = path_components.join("/");
        let mut path_copy = path.clone();
        path_copy.path = full_path;
        res.push(path_copy);
    }

    // Sort to make output easier to read
    res.sort_by(|a, b| a.path.cmp(&b.path));

    // Print information in a nice format roughly corresponding to MacOS's lsbom tool
    for file in res {
        print!("{:}\t{:o}\t{:}/{:}", file.path, file.mode, file.user, file.group);
        if file.kind == 1 {
            print!("\t{:}\t{:}", file.size, file.checksum);
        }
        println!();
    }
}
