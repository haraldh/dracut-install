use super::{Error, Header, SectionContent};
use std::collections::hash_map::HashMap;
use std::io::Read;

#[derive(Debug, Default, Clone)]
pub struct Strtab {
    hash: Option<HashMap<Vec<u8>, usize>>,
    data: Vec<u8>,
}

impl Strtab {
    pub fn from_reader<R>(
        mut io: R,
        _: Option<&SectionContent>,
        _: &Header,
    ) -> Result<SectionContent, Error>
    where
        R: Read,
    {
        let mut data = Vec::new();
        io.read_to_end(&mut data)?;
        Ok(SectionContent::Strtab(Strtab { hash: None, data }))
    }

    pub fn get(&self, i: usize) -> Vec<u8> {
        if i >= self.data.len() {
            println!("pointer {} into strtab extends beyond section size", i);
            return b"<corrupt>".to_vec();
        }
        self.data[i..]
            .split(|c| *c == 0)
            .next()
            .unwrap_or(&[0; 0])
            .to_vec()
    }
}
