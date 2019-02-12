use super::strtab::Strtab;
use super::utils::hextab;
use super::{types, Error, Header, SectionContent};
use num_traits::FromPrimitive;
use std::fmt;
use std::io::Read;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SymbolSectionIndex {
    Section(u16), // 1-6551
    Undefined,    // 0
    Absolute,     // 65521,
    Common,       // 6552,
}
impl Default for SymbolSectionIndex {
    fn default() -> SymbolSectionIndex {
        SymbolSectionIndex::Undefined
    }
}

#[derive(Default, Clone)]
pub struct Symbol {
    pub shndx: SymbolSectionIndex,
    pub value: u64,
    pub size: u64,

    pub name: Vec<u8>,
    pub stype: types::SymbolType,
    pub bind: types::SymbolBind,
    pub vis: types::SymbolVis,

    pub _name: u32,
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "  {} {:>5.5} {:<7.7} {:<6.6} {:<8.8} {:<3.3} {} ",
            hextab(16, self.value),
            self.size,
            format!("{:?}", self.stype),
            format!("{:?}", self.bind),
            format!("{:?}", self.vis),
            match self.shndx {
                SymbolSectionIndex::Undefined => String::from("UND"),
                SymbolSectionIndex::Absolute => String::from("ABS"),
                SymbolSectionIndex::Common => String::from("COM"),
                SymbolSectionIndex::Section(i) => format!("{}", i),
            },
            String::from_utf8_lossy(&self.name)
        )
    }
}

impl Symbol {
    fn from_val(
        tab: Option<&Strtab>,
        _name: u32,
        info: u8,
        other: u8,
        shndx: u16,
        value: u64,
        size: u64,
    ) -> Result<Symbol, Error> {
        let name = match tab {
            Some(tab) => tab.get(_name as usize),
            None => Vec::default(),
        };

        let shndx = match shndx {
            0 => SymbolSectionIndex::Undefined,
            65521 => SymbolSectionIndex::Absolute,
            65522 => SymbolSectionIndex::Common,
            _ if shndx > 0 && shndx < 6552 => SymbolSectionIndex::Section(shndx),
            _ => {
                return Err(Error::InvalidSymbolShndx(
                    String::from_utf8_lossy(&name).into_owned(),
                    shndx,
                ));
            }
        };

        let reb = info & 0xf;
        let stype = match types::SymbolType::from_u8(reb) {
            Some(v) => v,
            None => return Err(Error::InvalidSymbolType(reb)),
        };

        let reb = info >> 4;
        let bind = match types::SymbolBind::from_u8(reb) {
            Some(v) => v,
            None => return Err(Error::InvalidSymbolBind(reb)),
        };

        let reb = other & 0x3;
        let vis = match types::SymbolVis::from_u8(reb) {
            Some(v) => v,
            None => return Err(Error::InvalidSymbolVis(reb)),
        };

        Ok(Symbol {
            shndx,
            value,
            size,

            name,
            stype,
            bind,
            vis,

            _name,
        })
    }

    pub fn entsize(eh: &Header) -> usize {
        match eh.ident_class {
            types::Class::Class64 => 24,
            types::Class::Class32 => 16,
        }
    }

    pub fn from_reader<R>(
        mut io: R,
        linked: Option<&SectionContent>,
        eh: &Header,
    ) -> Result<SectionContent, Error>
    where
        R: Read,
    {
        let tab = match linked {
            None => None,
            Some(&SectionContent::Strtab(ref s)) => Some(s),
            any => {
                return Err(Error::LinkedSectionIsNotStrtab {
                    during: "reading symbols",
                    link: any.cloned(),
                });
            }
        };

        let mut r = Vec::new();
        let mut b = vec![0; Self::entsize(eh)];
        while io.read(&mut b)? > 0 {
            let mut br = &b[..];
            elf_dispatch_endianness!(eh => {
                let _name = read_u32(&mut br)?;
                r.push(match eh.ident_class {
                    types::Class::Class64 => {
                        let info = b[4];
                        let other = b[5];
                        br = &b[6..];
                        let shndx = read_u16(&mut br)?;
                        let value = read_u64(&mut br)?;
                        let size  = read_u64(&mut br)?;

                        Symbol::from_val(tab, _name, info, other, shndx, value, size)?
                    }
                    types::Class::Class32 => {
                        let value = read_u32(&mut br)?;
                        let size  = read_u32(&mut br)?;
                        let info  = b[12];
                        let other = b[13];
                        br = &b[14..];
                        let shndx = read_u16(&mut br)?;

                        Symbol::from_val(tab, _name, info, other, shndx, u64::from(value), u64::from(size))?
                    }
                })
            })
        }

        Ok(SectionContent::Symbols(r))
    }
}
