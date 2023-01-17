mod util;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::str;
use binary_parser::Binary;
use log::warn;

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

impl From<&[u8]> for Header {
    fn from(buf: &[u8]) -> Self {
        let mut bin = Binary::new(buf);
        Header {
            signature: bin.parse_bytes().unwrap(),
            version: bin.parse_u32_be().unwrap(),
            number_of_blocks: bin.parse_u32_be().unwrap(),
            index_offset: bin.parse_u32_be().unwrap(),
            index_length: bin.parse_u32_be().unwrap(),
            vars_offset: bin.parse_u32_be().unwrap(),
            vars_length: bin.parse_u32_be().unwrap(),
        }
    }
}

pub struct Pointer {
    pub address: u32,
    pub length: u32,
}

impl From<&[u8]> for Pointer {
    fn from(buf: &[u8]) -> Self {
        let mut bin = Binary::new(buf);
        Pointer {
            address: bin.parse_u32_be().unwrap(),
            length: bin.parse_u32_be().unwrap(),
        }
    }
}

#[derive(Debug)]
pub struct Var {
    pub index: u32,
    pub length: u8,
    pub name: String,
}

impl From<&[u8]> for Var {
    fn from(buf: &[u8]) -> Self {
        let mut bin = Binary::new(buf);
        let index = bin.parse_u32_be().unwrap();
        let length = bin.parse_u8().unwrap();
        Var {
            index,
            length,
            name: bin.parse_string(length as usize).unwrap(),
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

impl From<&[u8]> for Tree {
    fn from(buf: &[u8]) -> Self {
        let mut bin = Binary::new(buf);
        Tree {
            tree: bin.parse_bytes().unwrap(),
            version: bin.parse_u32_be().unwrap(),
            child: bin.parse_u32_be().unwrap(),
            block_size: bin.parse_u32_be().unwrap(),
            path_count: bin.parse_u32_be().unwrap(),
            unknown: bin.parse_u8().unwrap(),
        }
    }
}

pub struct TreeEntryIndices {
    pub value_index: u32,
    pub key_index: u32,
}

impl From<&[u8]> for TreeEntryIndices {
    fn from(buf: &[u8]) -> Self {
        let mut bin = Binary::new(buf);
        TreeEntryIndices {
            value_index: bin.parse_u32_be().unwrap(),
            key_index: bin.parse_u32_be().unwrap(),
        }
    }
}

pub struct TreeEntry {
    pub is_leaf: u16,
    pub count: u16,
    pub forward: u32,
    pub backward: u32,
}

impl From<&[u8]> for TreeEntry {
    fn from(buf: &[u8]) -> Self {
        let mut bin = Binary::new(buf);
        TreeEntry {
            is_leaf: bin.parse_u16_be().unwrap(),
            count: bin.parse_u16_be().unwrap(),
            forward: bin.parse_u32_be().unwrap(),
            backward: bin.parse_u32_be().unwrap(),
        }
    }
}

impl Bom {
    pub fn new(buffer: Vec<u8>) -> Self {
        let header = Header::from(&buffer[..]);
        let pointers = Self::parse_pointers(&buffer[header.index_offset as usize..]);
        let free_pointers_offset = header.index_offset as usize + 4 + pointers.len() * 8;
        let free_pointers = Self::parse_pointers(&buffer[free_pointers_offset..]);
        let variables = Self::parse_vars(&buffer[header.vars_offset as usize..]);
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

    fn parse_pointers(bytes: &[u8]) -> Vec<Pointer> {
        let mut bin = Binary::new(bytes);
        let pointer_count = bin.parse_u32_be().unwrap();
        let mut pointers = Vec::new();
        for _ in 0..pointer_count {
            let block = Pointer::from(bin.parse_buffer(8).unwrap());
            pointers.push(block);
        }
        pointers
    }

    fn parse_vars(bytes: &[u8]) -> HashMap<String, u32> {
        let mut bin = Binary::new(bytes);
        let var_count = bin.parse_u32_be().unwrap();
        let mut vars = HashMap::new();
        let mut pointer = 0;
        for _ in 0..var_count {
            let var = Var::from(bin.get_buffer(bin.position() + pointer, 1024).unwrap());
            pointer += var.length as usize + 5;
            vars.insert(var.name, var.index);
        }
        vars
    }

    pub fn pointer(&self, index: u32) -> Option<&Pointer> {
        self.pointers.get(index as usize)
    }

    pub fn pointer_for_var(&self, name: &str) -> Option<&Pointer> {
        self.variables.get(name).map(|index| self.pointer(*index)).flatten()
    }

    pub fn reduce_tree<'b, F, R>(&'b self, pointer_index: u32, initial_value: R, reduce: F) -> R
    where
        F: Fn(R, &'b [u8], &'b [u8]) -> R + Copy,
    {
        // Get the tree entry from the provided index
        let pointer = &self.pointer(pointer_index).unwrap();
        let mut bin = Binary::new(&self.buffer);
        bin.seek(pointer.address as usize);
        let entry = TreeEntry::from(bin.parse_buffer(12).unwrap());

        // Store initial value to reduce into
        let mut current_value = initial_value;

        if entry.is_leaf > 0 {
            // If it's a leaf then process the data
            for _ in 0..entry.count {
                // Each leaf has multiple entries which consist of a key and value pointer
                let indices = TreeEntryIndices::from(bin.parse_buffer(8).unwrap());
                // Get both the key and value pointers and check that they exist and are not empty
                // Corrupt files will cause out of bounds errors here otherwise
                match (self.pointer(indices.key_index), self.pointer(indices.value_index)) {
                    (Some(key_ptr), Some(value_ptr)) if key_ptr.length > 0 && value_ptr.length > 0 => {
                        current_value = reduce(
                            current_value,
                            bin.get_buffer(key_ptr.address as usize, key_ptr.length as usize).unwrap(),
                            bin.get_buffer(value_ptr.address as usize, value_ptr.length as usize).unwrap(),
                        );
                    }
                    _ => {
                        // We'll just skip parsing the leaf and warn about it
                        // It most likely means the asset catalog is corrupt
                        warn!("Invalid pointer: {} or {}", indices.key_index, indices.value_index);
                    }
                }
            }
        } else if entry.count == 0 {
            // The tree entry that's not a leaf should have no entries
            // TODO: Is this true though? Tere's a case of a weird asset catalog that has a count of more
            // but if trying to parse it will throw an exception
            // If not a leaf then get index of child pointer
            bin.seek((pointer.address + pointer.length) as usize); // TODO: maybe not needed?
            let index = bin.parse_u32_be().unwrap();
            current_value = self.reduce_tree(index, current_value, reduce);
        } else {
            warn!("Encountered a tree entry that's not a leaf and has entries (count: {})", entry.count);
        }

        // If has siblings then move horizontally to the next sibling
        if entry.forward != 0 {
            current_value = self.reduce_tree(entry.forward, current_value, reduce);
        }

        // Return accumulated value
        current_value
    }

    pub fn reduce_tree_for_variable<'b, F, R>(&'b self, var: &str, initial_value: R, reduce: F) -> Result<R, String>
    where
        F: Fn(R, &'b [u8], &'b [u8]) -> R + Copy,
    {
        match self.pointer_for_var(var) {
            Some(pointer) => {
                let tree = Tree::from(&self.buffer[pointer.address as usize..]);
                Ok(self.reduce_tree(tree.child, initial_value, reduce))
            }
            None => Err(format!("Variable not found: {}", var)),
        }
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
        let tree = Tree::from(&self.buffer[pointer.address as usize..]);
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
    }

   #[test]
   fn parsing_variables() {
         let bom = Bom::with_file("test_files/test.bom");
         let variables = bom.variables;
         assert_eq!(variables.len(), 5);
         assert_eq!(variables.get("Size64"), Some(&9));
         assert_eq!(variables.get("VIndex"), Some(&6));
         assert_eq!(variables.get("Paths"), Some(&2));
         assert_eq!(variables.get("BomInfo"), Some(&1));
         assert_eq!(variables.get("HLIndex"), Some(&4));
   } 

    #[test]
    fn reducing_tree() {
        let bom = Bom::with_file("test_files/test2.bom");
        let pointer = bom.pointer_for_var("Paths").unwrap();
        let tree = Tree::from(&bom.buffer[pointer.address as usize..]);
        let result = bom.reduce_tree(tree.child, 0, |reduction, _, _| reduction + 1);
        assert_eq!(result, 25);
    }

    #[test]
    fn mapping_tree() {
        let bom = Bom::with_file("test_files/test2.bom");
        let pointer = bom.pointer_for_var("Paths").unwrap();
        let tree = Tree::from(&bom.buffer[pointer.address as usize..]);
        let result = bom.map_tree(tree.child, |_, _| "test".to_string());
        assert_eq!(result.len(), 25);
        assert_eq!(result[0], "test".to_string());
    }
}
