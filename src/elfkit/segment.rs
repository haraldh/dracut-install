use super::error::Error;
use super::header::Header;
use super::types;

use num_traits::FromPrimitive;
use std::io::Read;

#[derive(Default, Debug, Clone)]
pub struct SegmentHeader {
    pub phtype: types::SegmentType,
    pub flags: types::SegmentFlags,
    pub offset: u64,
    pub vaddr: u64,
    pub paddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

impl SegmentHeader {
    /*    pub fn entsize(eh: &Header) -> usize {
            match eh.ident_class {
                types::Class::Class64 => 4 + 4 + 6 * 8,
                types::Class::Class32 => 4 + 4 + 6 * 4,
            }
        }
    */
    pub fn from_reader<R>(io: &mut R, eh: &Header) -> Result<SegmentHeader, Error>
    where
        R: Read,
    {
        let mut r = SegmentHeader::default();
        let reb = elf_read_u32!(eh, io)?;
        r.phtype = match types::SegmentType::from_u32(reb) {
            Some(v) => v,
            None => return Err(Error::InvalidSegmentType(reb)),
        };

        match eh.ident_class {
            types::Class::Class64 => {
                r.flags =
                    types::SegmentFlags::from_bits_truncate(u64::from(elf_read_u32!(eh, io)?));
                r.offset = elf_read_u64!(eh, io)?;
                r.vaddr = elf_read_u64!(eh, io)?;
                r.paddr = elf_read_u64!(eh, io)?;
                r.filesz = elf_read_u64!(eh, io)?;
                r.memsz = elf_read_u64!(eh, io)?;
                r.align = elf_read_u64!(eh, io)?;
            }
            types::Class::Class32 => {
                r.offset = u64::from(elf_read_u32!(eh, io)?);
                r.vaddr = u64::from(elf_read_u32!(eh, io)?);
                r.paddr = u64::from(elf_read_u32!(eh, io)?);
                r.filesz = u64::from(elf_read_u32!(eh, io)?);
                r.memsz = u64::from(elf_read_u32!(eh, io)?);
                r.flags =
                    types::SegmentFlags::from_bits_truncate(u64::from(elf_read_u32!(eh, io)?));
                r.align = u64::from(elf_read_u32!(eh, io)?);
            }
        };
        Ok(r)
    }
}
