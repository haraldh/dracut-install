#![allow(clippy::cast_lossless)]

use super::error::Error;
use super::header::Header;
use super::section::*;
use super::segment::*;

use std;
use std::io::{Read, Seek, SeekFrom};

#[derive(Default)]
pub struct Elf {
    pub header: Header,
    pub segments: Vec<SegmentHeader>,
    pub sections: Vec<Section>,
}

impl Elf {
    pub fn from_reader<R>(io: &mut R) -> Result<Elf, Error>
    where
        R: Read + Seek,
    {
        let header = Header::from_reader(io)?;

        // parse segments
        let mut segments = Vec::with_capacity(header.phnum as usize);
        io.seek(SeekFrom::Start(header.phoff))?;
        let mut buf = vec![0; header.phentsize as usize * header.phnum as usize];
        {
            io.read_exact(&mut buf)?;
            let mut bio = buf.as_slice();
            for _ in 0..header.phnum {
                let segment = SegmentHeader::from_reader(&mut bio, &header)?;
                segments.push(segment);
            }
        }

        // parse section headers
        let mut sections = Vec::with_capacity(header.shnum as usize);
        io.seek(SeekFrom::Start(header.shoff))?;
        buf.resize(header.shnum as usize * header.shentsize as usize, 0);
        {
            io.read_exact(&mut buf)?;
            let mut bio = buf.as_slice();
            for _ in 0..header.shnum {
                let sh = SectionHeader::from_reader(&mut bio, &header)?;

                sections.push(Section {
                    name: Vec::with_capacity(0),
                    content: SectionContent::Unloaded,
                    header: sh,
                    addrlock: true,
                });
            }
        }

        // resolve section names
        let shstrtab = match sections.get(header.shstrndx as usize) {
            None => return Err(Error::MissingShstrtabSection),
            Some(sec) => {
                io.seek(SeekFrom::Start(sec.header.offset))?;
                let mut shstrtab = vec![0; sec.header.size as usize];
                io.read_exact(&mut shstrtab)?;
                shstrtab
            }
        };

        for sec in sections.iter_mut() {
            sec.name = shstrtab[sec.header.name as usize..]
                .split(|e| *e == 0)
                .next()
                .unwrap_or(&[0; 0])
                .to_vec();
        }

        Ok(Elf {
            header,
            segments,
            sections,
        })
    }

    pub fn load<R>(&mut self, i: usize, io: &mut R) -> Result<(), Error>
    where
        R: Read + Seek,
    {
        let mut sec = std::mem::replace(&mut self.sections[i], Section::default());
        {
            let link = sec.header.link;
            let linked = {
                if link < 1 || link as usize >= self.sections.len() {
                    None
                } else {
                    self.load(link as usize, io)?;
                    Some(&self.sections[link as usize])
                }
            };
            sec.from_reader(io, linked, &self.header)?;
        }
        self.sections[i] = sec;

        Ok(())
    }
}
