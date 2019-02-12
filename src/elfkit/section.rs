use super::dynamic::Dynamic;
use super::error::Error;
use super::header::Header;
use super::relocation::Relocation;
use super::strtab::Strtab;
use super::symbol::Symbol;
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
    /*
        pub fn entsize(eh: &Header) -> usize {
            4 + 4
                + match eh.ident_class {
                    types::Class::Class64 => 6 * 8,
                    types::Class::Class32 => 6 * 4,
                }
                + 4
                + 4
        }
    */
    pub fn from_reader<R>(io: &mut R, eh: &Header) -> Result<SectionHeader, Error>
    where
        R: Read,
    {
        elf_dispatch_endianness!(eh => {
            let mut r = SectionHeader::default();
            r.name   = read_u32(io)?;
            let reb  = read_u32(io)?;
            r.shtype = types::SectionType(reb);

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
    Raw(Vec<u8>),
    Relocations(Vec<Relocation>),
    Symbols(Vec<Symbol>),
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
    pub fn as_symbols(&self) -> Option<&Vec<Symbol>> {
        match *self {
            SectionContent::Symbols(ref v) => Some(v),
            _ => None,
        }
    }
    pub fn as_symbols_mut(&mut self) -> Option<&mut Vec<Symbol>> {
        match *self {
            SectionContent::Symbols(ref mut v) => Some(v),
            _ => None,
        }
    }
    pub fn into_symbols(self) -> Option<Vec<Symbol>> {
        match self {
            SectionContent::Symbols(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_relocations(&self) -> Option<&Vec<Relocation>> {
        match *self {
            SectionContent::Relocations(ref v) => Some(v),
            _ => None,
        }
    }
    pub fn as_relocations_mut(&mut self) -> Option<&mut Vec<Relocation>> {
        match *self {
            SectionContent::Relocations(ref mut v) => Some(v),
            _ => None,
        }
    }
    pub fn into_relocations(self) -> Option<Vec<Relocation>> {
        match self {
            SectionContent::Relocations(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_raw(&self) -> Option<&Vec<u8>> {
        match *self {
            SectionContent::Raw(ref v) => Some(v),
            _ => None,
        }
    }
    pub fn as_raw_mut(&mut self) -> Option<&mut Vec<u8>> {
        match *self {
            SectionContent::Raw(ref mut v) => Some(v),
            _ => None,
        }
    }
    pub fn into_raw(self) -> Option<Vec<u8>> {
        match self {
            SectionContent::Raw(v) => Some(v),
            _ => None,
        }
    }
    /*
    pub fn size(&self, eh: &Header) -> usize {
        match *self {
            SectionContent::Unloaded => panic!("cannot size unloaded section"),
            SectionContent::None => 0,
            SectionContent::Raw(ref v) => v.len(),
            SectionContent::Dynamic(ref v) => v.len() * Dynamic::entsize(eh),
            SectionContent::Strtab(ref v) => v.len(eh),
            SectionContent::Symbols(ref v) => v.len() * Symbol::entsize(eh),
            SectionContent::Relocations(ref v) => v.len() * Relocation::entsize(eh),
        }
    }
    */
}

#[derive(Debug, Default, Clone)]
pub struct Section {
    pub header: SectionHeader,
    pub name: Vec<u8>,
    pub content: SectionContent,
    pub addrlock: bool,
}

impl Section {
    /*
        pub fn size(&self, eh: &Header) -> usize {
            self.content.size(eh)
        }

        pub fn new(
            name: Vec<u8>,
            shtype: types::SectionType,
            flags: types::SectionFlags,
            content: SectionContent,
            link: u32,
            info: u32,
        ) -> Section {
            Section {
                name,
                header: SectionHeader {
                    name: 0,
                    shtype,
                    flags,
                    addr: 0,
                    offset: 0,
                    size: 0,
                    link,
                    info,
                    addralign: 0,
                    entsize: 0,
                },
                content,
                addrlock: false,
            }
        }
    */

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
            types::SectionType::NOBITS => {
                unreachable!();
            }
            types::SectionType::STRTAB => {
                let io = bb.as_slice();
                Strtab::from_reader(io, linked, eh)?
            }
            types::SectionType::RELA => {
                let io = bb.as_slice();
                Relocation::from_reader(io, linked, eh)?
            }
            types::SectionType::SYMTAB | types::SectionType::DYNSYM => {
                let io = bb.as_slice();
                Symbol::from_reader(io, linked, eh)?
            }
            types::SectionType::DYNAMIC => {
                let io = bb.as_slice();
                Dynamic::from_reader(io, linked, eh)?
            }
            _ => SectionContent::Raw(bb),
        };
        Ok(())
    }
}
