use super::error::Error;
use num_traits::FromPrimitive;

use super::types;
use std::io::Read;

#[derive(Debug, Clone)]
pub struct Header {
    pub ident_magic: [u8; 4],
    pub ident_class: types::Class,
    pub ident_endianness: types::Endianness,
    pub ident_version: u8, // 1
    pub ident_abi: types::Abi,
    pub ident_abiversion: u8,

    pub etype: types::ElfType,
    pub machine: types::Machine,
    pub version: u32, //1
    pub entry: u64,   //program counter starts here
    pub phoff: u64,   //offset of program header table
    pub shoff: u64,   //offset of section header table
    pub flags: types::HeaderFlags,
    pub ehsize: u16,    //size of this header (who cares?)
    pub phentsize: u16, //the size of a program header table entry
    pub phnum: u16,     //the number of entries in the program header table
    pub shentsize: u16, //the size of a section header table entry
    pub shnum: u16,     //the number of entries in the section header table
    pub shstrndx: u16,  //where to find section names
}

impl Default for Header {
    fn default() -> Self {
        Header {
            ident_magic: [0x7F, 0x45, 0x4c, 0x46],
            ident_class: types::Class::Class64,
            ident_endianness: types::Endianness::LittleEndian,
            ident_version: 1,
            ident_abi: types::Abi::SYSV,
            ident_abiversion: 0,
            etype: types::ElfType::default(),
            machine: types::Machine::default(),
            version: 1,
            entry: 0,
            phoff: 0,
            shoff: 0,
            flags: types::HeaderFlags::default(),
            ehsize: 0,
            phentsize: 0,
            phnum: 0,
            shentsize: 0,
            shnum: 0,
            shstrndx: 0,
        }
    }
}

impl Header {
    pub fn from_reader<R>(io: &mut R) -> Result<Header, Error>
    where
        R: Read,
    {
        let mut r = Header::default();
        let mut b = [0; 16];
        if io.read_exact(&mut b).is_err() {
            return Err(Error::InvalidMagic);
        }
        r.ident_magic.clone_from_slice(&b[0..4]);

        if r.ident_magic != [0x7F, 0x45, 0x4c, 0x46] {
            return Err(Error::InvalidMagic);
        }

        r.ident_class = match types::Class::from_u8(b[4]) {
            Some(v) => v,
            None => return Err(Error::InvalidIdentClass(b[4])),
        };

        r.ident_endianness = match types::Endianness::from_u8(b[5]) {
            Some(v) => v,
            None => return Err(Error::InvalidEndianness(b[5])),
        };

        r.ident_version = b[6];
        if r.ident_version != 1 {
            return Err(Error::InvalidIdentVersion(b[6]));
        }

        r.ident_abi = match types::Abi::from_u8(b[7]) {
            Some(v) => v,
            None => return Err(Error::InvalidAbi(b[7])),
        };

        r.ident_abiversion = b[8];

        elf_dispatch_endianness!(r => {

            let reb = read_u16(io)?;
            r.etype = match types::ElfType::from_u16(reb) {
                Some(v) => v,
                None => return Err(Error::InvalidElfType(reb)),
            };

            let reb = read_u16(io)?;
            r.machine = match types::Machine::from_u16(reb) {
                Some(v) => v,
                None => return Err(Error::InvalidMachineType(reb)),
            };

            r.version = read_u32(io)?;
            if r.version != 1 {
                return Err(Error::InvalidVersion(r.version));
            }


            elf_dispatch_uclass!(r => {
                r.entry = read_uclass(io)?;
                r.phoff = read_uclass(io)?;
                r.shoff = read_uclass(io)?;
            });

            let reb = io.read_u32::<DispatchedEndian>()?;
            r.flags = types::HeaderFlags::from_bits_truncate(reb);
            //r.flags = match types::HeaderFlags::from_bits(reb) {
            //    Some(v) => v,
            //    None => return Err(Error::InvalidHeaderFlags(reb)),
            //};

            r.ehsize    = read_u16(io)?;
            r.phentsize = read_u16(io)?;
            r.phnum     = read_u16(io)?;
            r.shentsize = read_u16(io)?;
            r.shnum     = read_u16(io)?;
            r.shstrndx  = read_u16(io)?;
        });

        Ok(r)
    }
}
