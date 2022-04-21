mod util;

use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::mem;
use std::str;

pub struct Bom {
    pub buffer: Vec<u8>,
    header: BomHeader,
    pointers: Vec<BomPointer>,
    free_pointers: Vec<BomPointer>,
    variables: HashMap<String, u32>,
}

#[repr(C)]
#[derive(Deserialize)]
pub struct BomHeader {
    pub signature: [u8; 8],
    pub version: u32,
    pub number_of_blocks: u32,
    pub index_offset: u32,
    pub index_length: u32,
    pub vars_offset: u32,
    pub vars_length: u32,
}

#[repr(C)]
#[derive(Deserialize)]
pub struct BomPointer {
    pub address: u32,
    pub length: u32,
}

#[repr(C)]
#[derive(Deserialize, Debug)]
pub struct BomVar {
    pub index: u32,
    pub length: u8,
    #[serde(skip_deserializing)]
    pub name: String,
}

#[repr(C)]
#[derive(Deserialize, Debug)]
pub struct BomTree {
    pub tree: [u8; 4],
    pub version: u32,
    pub child: u32,
    pub block_size: u32,
    pub path_count: u32,
    unknown: u8,
}

#[repr(C)]
#[derive(Deserialize, Debug)]
pub struct BomTreeEntryIndices {
    pub value_index: u32,
    pub key_index: u32,
}

#[repr(C)]
#[derive(Deserialize, Debug)]
pub struct BomTreeEntry {
    pub is_leaf: u16,
    pub count: u16,
    pub forward: u32,
    pub backward: u32,
}

impl Bom {
    pub fn new(buffer: Vec<u8>) -> Self {
        let header: BomHeader = util::decode_bytes_be(&buffer[..]);
        let pointers = Self::parse_pointers(&buffer, header.index_offset);
        let free_pointers_offset = header.index_offset as usize + 4 + pointers.len() * 8;
        let free_pointers = Self::parse_pointers(&buffer, free_pointers_offset as u32);
        let variables = Self::parse_vars(&buffer, header.vars_offset);
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

    fn parse_pointers(buffer: &Vec<u8>, offset: u32) -> Vec<BomPointer> {
        let pointer_count = util::decode_bytes_be::<u32>(&buffer[offset as usize..]);
        let mut pointers = Vec::new();
        for i in 0..pointer_count {
            let block = util::decode_bytes_be(&buffer[(offset + 4 + i * 8) as usize..]);
            pointers.push(block);
        }
        pointers
    }

    fn parse_vars(buffer: &Vec<u8>, offset: u32) -> HashMap<String, u32> {
        let var_count = util::decode_bytes_be(&buffer[offset as usize..]);
        let mut vars = HashMap::new();
        let mut pointer = offset + 4;
        for _ in 0..var_count {
            let mut var = util::decode_bytes_be::<BomVar>(&buffer[pointer as usize..]);
            pointer += 5;
            let name =
                str::from_utf8(&buffer[pointer as usize..pointer as usize + var.length as usize])
                    .unwrap();
            pointer += var.length as u32;
            var.name = name.to_string();
            vars.insert(var.name, var.index);
        }
        vars
    }

    pub fn pointer(&self, index: u32) -> &BomPointer {
        &self.pointers[index as usize]
    }

    pub fn pointer_for_var(&self, name: &str) -> Option<&BomPointer> {
        self.variables.get(name).map(|index| self.pointer(*index))
    }

    pub fn reduce_tree<'b, F, R>(&'b self, pointer_index: u32, initial_value: R, reduce: F) -> R
    where
        F: Fn(R, &'b [u8], &'b [u8]) -> R + Copy,
    {
        // Get the tree from the provided index
        let pointer = &self.pointers[pointer_index as usize];
        let entry = util::decode_bytes_be::<BomTreeEntry>(&self.buffer[pointer.address as usize..]);

        // Store initial value to reduce into
        let mut current_value = initial_value;

        if entry.is_leaf > 0 {
            // If it's a leaf then process the data
            for i in 0..entry.count {
                // Each leaf has multiple entries which consist of a key and value pointer
                let indices = util::decode_bytes_be::<BomTreeEntryIndices>(
                    &self.buffer[(pointer.address as usize)
                        + mem::size_of::<BomTreeEntry>()
                        + (i as usize * 8)..],
                );
                let key_ptr = &self.pointers[indices.key_index as usize];
                let value_ptr = &self.pointers[indices.value_index as usize];
                current_value = reduce(current_value,
                    &self.buffer[key_ptr.address as usize
                        ..key_ptr.address as usize + key_ptr.length as usize],
                    &self.buffer[value_ptr.address as usize
                        ..value_ptr.address as usize + value_ptr.length as usize],
                );
            }
        } else {
            // If not a leaf then get index of child pointer
            let index = util::decode_bytes_be::<u32>(
                &self.buffer[(pointer.address + pointer.length) as usize..],
            );
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
        let tree = util::decode_bytes_be::<BomTree>(&self.buffer[pointer.address as usize..]);
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
        let tree = util::decode_bytes_be::<BomTree>(&self.buffer[pointer.address as usize..]);
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

impl fmt::Debug for BomHeader {
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

impl fmt::Debug for BomPointer {
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
        let tree = util::decode_bytes_be::<BomTree>(&bom.buffer[pointer.address as usize..]);
        let result = bom.reduce_tree(tree.child, 0, |reduction, _, _| {
            reduction + 1
        });
        assert_eq!(result, 25);
    }

    #[test]
    fn mapping_tree() {
        let bom = Bom::with_file("test_files/test2.bom");
        let pointer = bom.pointer_for_var("Paths").unwrap();
        let tree = util::decode_bytes_be::<BomTree>(&bom.buffer[pointer.address as usize..]);
        let result = bom.map_tree(tree.child, |_, _| {
            "test".to_string()
        });
        assert_eq!(result.len(), 25);
        assert_eq!(result[0], "test".to_string());
    }
}
