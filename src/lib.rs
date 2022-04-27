mod util;

use bytes::{Buf, Bytes};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::str;
use util::{GetBytes, GetString};

pub struct Bom {
    pub buffer: Vec<u8>,
    header: Header,
    pointers: Vec<Pointer>,
    free_pointers: Vec<Pointer>,
    variables: HashMap<String, u32>,
}

pub struct Header {
    pub signature: [u8; 8],
    pub version: u32,
    pub number_of_blocks: u32,
    pub index_offset: u32,
    pub index_length: u32,
    pub vars_offset: u32,
    pub vars_length: u32,
}

impl<T: Buf> From<T> for Header {
    fn from(mut buf: T) -> Self {
        Header {
            signature: buf.get_bytes(),
            version: buf.get_u32(),
            number_of_blocks: buf.get_u32(),
            index_offset: buf.get_u32(),
            index_length: buf.get_u32(),
            vars_offset: buf.get_u32(),
            vars_length: buf.get_u32(),
        }
    }
}

pub struct Pointer {
    pub address: u32,
    pub length: u32,
}

impl<T: Buf> From<T> for Pointer {
    fn from(mut buf: T) -> Self {
        Pointer {
            address: buf.get_u32(),
            length: buf.get_u32(),
        }
    }
}

#[derive(Debug)]
pub struct Var {
    pub index: u32,
    pub length: u8,
    pub name: String,
}

impl<T: Buf> From<T> for Var {
    fn from(mut buf: T) -> Self {
        let i = buf.get_u32();
        let length = buf.get_u8();
        Var {
            index: i,
            length: length,
            name: buf.get_string(length as usize).unwrap(),
        }
    }
}

pub struct Tree {
    pub tree: [u8; 4],
    pub version: u32,
    pub child: u32,
    pub block_size: u32,
    pub path_count: u32,
    #[allow(dead_code)]
    unknown: u8,
}

impl<T: Buf> From<T> for Tree {
    fn from(mut buf: T) -> Self {
        Tree {
            tree: buf.get_bytes(),
            version: buf.get_u32(),
            child: buf.get_u32(),
            block_size: buf.get_u32(),
            path_count: buf.get_u32(),
            unknown: buf.get_u8(),
        }
    }
}

pub struct TreeEntryIndices {
    pub value_index: u32,
    pub key_index: u32,
}

impl<T: Buf> From<T> for TreeEntryIndices {
    fn from(mut buf: T) -> Self {
        TreeEntryIndices {
            value_index: buf.get_u32(),
            key_index: buf.get_u32(),
        }
    }
}

pub struct TreeEntry {
    pub is_leaf: u16,
    pub count: u16,
    pub forward: u32,
    pub backward: u32,
}

impl<T: Buf> From<T> for TreeEntry {
    fn from(mut buf: T) -> Self {
        TreeEntry {
            is_leaf: buf.get_u16(),
            count: buf.get_u16(),
            forward: buf.get_u32(),
            backward: buf.get_u32(),
        }
    }
}

impl Bom {
    pub fn new(buffer: Vec<u8>) -> Self {
        let bytes = Bytes::from(buffer.clone());
        let header = Header::from(bytes.clone());
        let pointers = Self::parse_pointers(bytes.slice(header.index_offset as usize..));
        let free_pointers_offset = header.index_offset as usize + 4 + pointers.len() * 8;
        let free_pointers = Self::parse_pointers(bytes.slice(free_pointers_offset..));
        let variables = Self::parse_vars(bytes.slice(header.vars_offset as usize..));
        Bom {
            buffer,
            header,
            pointers,
            free_pointers,
            variables,
        }
    }

    pub fn with_file(path: &str) -> Self {
        Self::new(fs::read(path).unwrap())
    }

    fn parse_pointers(mut bytes: Bytes) -> Vec<Pointer> {
        let pointer_count = bytes.get_u32();
        let mut pointers = Vec::new();
        for _ in 0..pointer_count {
            let block = Pointer::from(bytes.copy_to_bytes(8));
            pointers.push(block);
        }
        pointers
    }

    fn parse_vars(mut bytes: Bytes) -> HashMap<String, u32> {
        let var_count = bytes.get_u32();
        let mut vars = HashMap::new();
        let mut pointer = 0;
        for _ in 0..var_count {
            let var = Var::from(bytes.slice(pointer..));
            pointer += var.length as usize + 5;
            vars.insert(var.name, var.index);
        }
        vars
    }

    pub fn pointer(&self, index: u32) -> &Pointer {
        &self.pointers[index as usize]
    }

    pub fn pointer_for_var(&self, name: &str) -> Option<&Pointer> {
        self.variables.get(name).map(|index| self.pointer(*index))
    }

    pub fn reduce_tree<'b, F, R>(&'b self, pointer_index: u32, initial_value: R, reduce: F) -> R
    where
        F: Fn(R, &'b [u8], &'b [u8]) -> R + Copy,
    {
        let bytes = Bytes::from(self.buffer.to_vec());

        // Get the tree from the provided index
        let pointer = &self.pointers[pointer_index as usize];
        let entry = TreeEntry::from(bytes.slice(pointer.address as usize..));

        // Store initial value to reduce into
        let mut current_value = initial_value;

        if entry.is_leaf > 0 {
            let mut bytes = bytes.slice(pointer.address as usize + 12..);
            // If it's a leaf then process the data
            for _ in 0..entry.count {
                // Each leaf has multiple entries which consist of a key and value pointer
                let indices = TreeEntryIndices::from(bytes.copy_to_bytes(8));
                let key_ptr = &self.pointers[indices.key_index as usize];
                let value_ptr = &self.pointers[indices.value_index as usize];
                current_value = reduce(
                    current_value,
                    &self.buffer[key_ptr.address as usize
                        ..key_ptr.address as usize + key_ptr.length as usize],
                    &self.buffer[value_ptr.address as usize
                        ..value_ptr.address as usize + value_ptr.length as usize],
                );
            }
        } else {
            // If not a leaf then get index of child pointer
            let index = bytes
                .slice((pointer.address + pointer.length) as usize..)
                .get_u32();
            current_value = self.reduce_tree(index, current_value, reduce);
        }

        // If has siblings then move horizontally to the next sibling
        if entry.forward != 0 {
            current_value = self.reduce_tree(entry.forward, current_value, reduce);
        }

        // Return accumulated value
        current_value
    }

    pub fn reduce_tree_for_variable<'b, F, R>(&'b self, var: &str, initial_value: R, reduce: F) -> R
    where
        F: Fn(R, &'b [u8], &'b [u8]) -> R + Copy,
    {
        let pointer = self.pointer_for_var(var).unwrap();
        let bytes = Bytes::from(self.buffer.to_vec()).slice(pointer.address as usize..);
        let tree = Tree::from(bytes);
        self.reduce_tree(tree.child, initial_value, reduce)
    }

    pub fn map_tree<'b, F, V>(&'b self, pointer_index: u32, map: F) -> Vec<V>
    where
        F: Fn(&'b [u8], &'b [u8]) -> V + Copy,
    {
        self.reduce_tree(pointer_index, Vec::new(), |mut acc, key, value| {
            acc.push(map(key, value));
            acc
        })
    }

    pub fn map_tree_for_variable<'b, F, V>(&'b self, var: &str, map: F) -> Vec<V>
    where
        F: Fn(&'b [u8], &'b [u8]) -> V + Copy,
    {
        let pointer = self.pointer_for_var(var).unwrap();
        let bytes = Bytes::from(self.buffer.to_vec()).slice(pointer.address as usize..);
        let tree = Tree::from(bytes);
        self.map_tree(tree.child, map)
    }
}

impl fmt::Debug for Bom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Bom")
            .field("header", &self.header)
            .field("pointers", &self.pointers)
            .field("free_pointers", &self.free_pointers)
            .field("variables", &self.variables)
            .finish()
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BomHeader")
            .field("signature", &util::format_hex(&self.signature))
            .field("version", &self.version)
            .field("number_of_blocks", &self.number_of_blocks)
            .field("index_offset", &self.index_offset)
            .field("index_length", &self.index_length)
            .field("vars_offset", &self.vars_offset)
            .field("vars_length", &self.vars_length)
            .finish()
    }
}

impl fmt::Debug for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "BomPointer {{ address: {}, length: {} }}",
            self.address, self.length
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn load_with_error() {
        Bom::with_file("test_files/bleh.car");
    }

    #[test]
    fn parsing_car() {
        let car = Bom::with_file("test_files/assets.car");
        assert_eq!(car.header.signature, [66, 79, 77, 83, 116, 111, 114, 101]);
        assert_eq!(car.header.version, 1);
        assert_eq!(car.header.number_of_blocks, 62);
        assert_eq!(car.header.index_offset, 9872);
        assert_eq!(car.header.index_length, 2088);
        assert_eq!(car.header.vars_offset, 48256);
        assert_eq!(car.header.vars_length, 117);
        assert_eq!(car.pointers.len(), 256);
        assert_eq!(car.free_pointers.len(), 3);
        assert_eq!(car.variables.len(), 7);
        assert_eq!(car.variables.get("CARHEADER"), Some(&1));
        assert_eq!(car.variables.get("RENDITIONS"), Some(&2));
        assert_eq!(car.variables.get("FACETKEYS"), Some(&4));
        assert_eq!(car.variables.get("APPEARANCEKEYS"), Some(&6));
        assert_eq!(car.variables.get("KEYFORMAT"), Some(&24));
        assert_eq!(car.variables.get("EXTENDED_METADATA"), Some(&53));
        assert_eq!(car.variables.get("BITMAPKEYS"), Some(&54));
    }

    #[test]
    fn parsing_bom() {
        let bom = Bom::with_file("test_files/test.bom");
        assert_eq!(bom.header.signature, [66, 79, 77, 83, 116, 111, 114, 101]);
        assert_eq!(bom.header.version, 1);
        assert_eq!(bom.header.number_of_blocks, 28);
        assert_eq!(bom.header.index_offset, 13455);
        assert_eq!(bom.header.index_length, 21896);
        assert_eq!(bom.header.vars_offset, 8994);
        assert_eq!(bom.header.vars_length, 60);
        assert_eq!(bom.pointers.len(), 2730);
        assert_eq!(bom.free_pointers.len(), 4);
        assert_eq!(bom.variables.len(), 5);
        assert_eq!(bom.variables.get("Size64"), Some(&9));
        assert_eq!(bom.variables.get("VIndex"), Some(&6));
        assert_eq!(bom.variables.get("Paths"), Some(&2));
        assert_eq!(bom.variables.get("BomInfo"), Some(&1));
        assert_eq!(bom.variables.get("HLIndex"), Some(&4));
    }

    #[test]
    fn reducing_tree() {
        let bom = Bom::with_file("test_files/test2.bom");
        let pointer = bom.pointer_for_var("Paths").unwrap();
        let bytes = Bytes::from(bom.buffer.to_vec()).slice(pointer.address as usize..);
        let tree = Tree::from(bytes);
        let result = bom.reduce_tree(tree.child, 0, |reduction, _, _| reduction + 1);
        assert_eq!(result, 25);
    }

    #[test]
    fn mapping_tree() {
        let bom = Bom::with_file("test_files/test2.bom");
        let pointer = bom.pointer_for_var("Paths").unwrap();
        let bytes = Bytes::from(bom.buffer.to_vec()).slice(pointer.address as usize..);
        let tree = Tree::from(bytes);
        let result = bom.map_tree(tree.child, |_, _| "test".to_string());
        assert_eq!(result.len(), 25);
        assert_eq!(result[0], "test".to_string());
    }
}
