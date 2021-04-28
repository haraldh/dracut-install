use super::dynamic::Dynamic;
use super::error::Error;
use super::header::Header;
use super::strtab::Strtab;
use super::types;

use std::io::{Read, Seek, SeekFrom};

#[derive(Default, Debug, Clone)]
pub struct SectionHeader {
    pub name: u32,
    pub shtype: types::SectionType,
    pub flags: types::SectionFlags,
    pub addr: u64,
    pub offset: u64,
    pub size: u64,
    pub link: u32,
    pub info: u32,
    pub addralign: u64,
    pub entsize: u64,
}

impl SectionHeader {
    pub fn from_reader<R>(io: &mut R, eh: &Header) -> Result<SectionHeader, Error>
    where
        R: Read,
    {
        elf_dispatch_endianness!(eh => {
            let name   = read_u32(io)?;
            let reb  = read_u32(io)?;

            let mut r = SectionHeader {
                name,
                shtype: types::SectionType(reb),
                ..Default::default()
            };

            elf_dispatch_uclass!(eh => {
                let reb = read_uclass(io)?;
                r.flags = match types::SectionFlags::from_bits(reb) {
                    Some(v) => v,
                    None => return Err(Error::InvalidSectionFlags(reb)),
                };
                r.addr   = read_uclass(io)?;
                r.offset = read_uclass(io)?;
                r.size   = read_uclass(io)?;
                r.link   = read_u32(io)?;
                r.info   = read_u32(io)?;
                r.addralign = read_uclass(io)?;
                r.entsize = read_uclass(io)?;
                Ok(r)
            })
        })
    }
}

#[derive(Debug, Clone)]
pub enum SectionContent {
    None,
    Unloaded,
    Dynamic(Vec<Dynamic>),
    Strtab(Strtab),
}

impl Default for SectionContent {
    fn default() -> Self {
        SectionContent::None
    }
}

#[allow(dead_code)]
impl SectionContent {
    pub fn as_dynamic_mut(&mut self) -> Option<&mut Vec<Dynamic>> {
        match *self {
            SectionContent::Dynamic(ref mut v) => Some(v),
            _ => None,
        }
    }
    pub fn as_dynamic(&self) -> Option<&Vec<Dynamic>> {
        match *self {
            SectionContent::Dynamic(ref v) => Some(v),
            _ => None,
        }
    }
    pub fn into_dynamic(self) -> Option<Vec<Dynamic>> {
        match self {
            SectionContent::Dynamic(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_strtab_mut(&mut self) -> Option<&mut Strtab> {
        match *self {
            SectionContent::Strtab(ref mut v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Section {
    pub header: SectionHeader,
    pub name: Vec<u8>,
    pub content: SectionContent,
    pub addrlock: bool,
}

impl Section {
    #[allow(clippy::wrong_self_convention)]
    pub fn from_reader<T>(
        &mut self,
        mut io: T,
        linked: Option<&Section>,
        eh: &Header,
    ) -> Result<(), Error>
    where
        T: Read + Seek,
    {
        match self.content {
            SectionContent::Unloaded => {}
            _ => return Ok(()),
        };
        if self.header.shtype == types::SectionType::NOBITS {
            self.content = SectionContent::None;
            return Ok(());
        };
        io.seek(SeekFrom::Start(self.header.offset))?;
        let mut bb = vec![0; self.header.size as usize];
        io.read_exact(&mut bb)?;
        let linked = linked.map(|s| &s.content);
        self.content = match self.header.shtype {
            types::SectionType::STRTAB => {
                let io = bb.as_slice();
                Strtab::from_reader(io, linked, eh)?
            }
            types::SectionType::DYNAMIC => {
                let io = bb.as_slice();
                Dynamic::from_reader(io, linked, eh)?
            }
            _ => SectionContent::Unloaded,
        };
        Ok(())
    }
}
