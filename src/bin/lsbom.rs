use rbom::*;
use std::env;
use bytes::{Bytes, Buf};
use std::collections::HashMap;

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

fn read_file_info(buffer: &[u8]) -> File {
    let mut file = File {
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
    };

    // We're copying here, not ideal, but we're trying this for now.
    let mut bytes = Bytes::from(buffer.to_vec());
    file.kind = bytes.get_u8();
    let _ = bytes.get_u8(); // unknown
    file.architecture = bytes.get_u16();
    file.mode = bytes.get_u16();
    file.user = bytes.get_u32();
    file.group = bytes.get_u32();
    file.modtime = bytes.get_u32();
    file.size = bytes.get_u32();
    let _ = bytes.get_u8(); // unknown
    file.checksum = bytes.get_u32();

    file
}

pub fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: lsbom <bom_file>");
        return;
    }

    let bom = Bom::with_file(&args[1]);

    // Extract the file infos
    let paths = bom.reduce_tree_for_variable("Paths", HashMap::new(), |mut initial, key, val| {
        let id = u32::from_be_bytes(val[0..4].try_into().unwrap());
        let index = u32::from_be_bytes(val[4..8].try_into().unwrap());
        let parent = u32::from_be_bytes(key[0..4].try_into().unwrap());
        let path = std::str::from_utf8(&key[4..]).unwrap().trim_end_matches('\0');
        let file_info_ptr = bom.pointer(index);
        let file_info_buf = &bom.buffer[file_info_ptr.address as usize..file_info_ptr.address as usize + file_info_ptr.length as usize];
        let mut file_info = read_file_info(file_info_buf);
        file_info.parent = parent;
        file_info.path = path.to_string();
        initial.insert(id, file_info);
        initial
    });

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
