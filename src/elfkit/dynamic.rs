use super::types;
use super::{Error, Header, SectionContent};
use num_traits::FromPrimitive;
use std::io::Read;

#[derive(Debug, Clone)]
pub enum DynamicContent {
    None,
    String((Vec<u8>, Option<u64>)),
    Address(u64),
    Flags1(types::DynamicFlags1),
}

impl Default for DynamicContent {
    fn default() -> Self {
        DynamicContent::None
    }
}

#[derive(Debug, Clone, Default)]
pub struct Dynamic {
    pub dhtype: types::DynamicType,
    pub content: DynamicContent,
}

impl Dynamic {
    pub fn from_reader<R>(
        mut io: R,
        linked: Option<&SectionContent>,
        eh: &Header,
    ) -> Result<SectionContent, Error>
    where
        R: Read,
    {
        let strtab = match linked {
            None => None,
            Some(&SectionContent::Strtab(ref s)) => Some(s),
            any => {
                return Err(Error::LinkedSectionIsNotStrtab {
                    during: "reading dynamic",
                    link: any.cloned(),
                });
            }
        };

        let mut r = Vec::new();

        while let Ok(tag) = elf_read_uclass!(eh, io) {
            let val = elf_read_uclass!(eh, io)?;

            match types::DynamicType::from_u64(tag) {
                None => return Err(Error::InvalidDynamicType(tag)),
                Some(types::DynamicType::NULL) => {
                    r.push(Dynamic {
                        dhtype: types::DynamicType::NULL,
                        content: DynamicContent::None,
                    });
                    break;
                }
                Some(types::DynamicType::RPATH) => {
                    r.push(Dynamic {
                        dhtype: types::DynamicType::RPATH,
                        content: DynamicContent::String(match strtab {
                            None => (Vec::default(), None),
                            Some(s) => (s.get(val as usize), Some(val)),
                        }),
                    });
                }
                Some(types::DynamicType::RUNPATH) => {
                    r.push(Dynamic {
                        dhtype: types::DynamicType::RUNPATH,
                        content: DynamicContent::String(match strtab {
                            None => (Vec::default(), None),
                            Some(s) => (s.get(val as usize), Some(val)),
                        }),
                    });
                }
                Some(types::DynamicType::NEEDED) => {
                    r.push(Dynamic {
                        dhtype: types::DynamicType::NEEDED,
                        content: DynamicContent::String(match strtab {
                            None => (Vec::default(), None),
                            Some(s) => (s.get(val as usize), Some(val)),
                        }),
                    });
                }
                Some(types::DynamicType::FLAGS_1) => {
                    r.push(Dynamic {
                        dhtype: types::DynamicType::FLAGS_1,
                        content: DynamicContent::Flags1(
                            match types::DynamicFlags1::from_bits(val) {
                                Some(v) => v,
                                None => return Err(Error::InvalidDynamicFlags1(val)),
                            },
                        ),
                    });
                }
                Some(x) => {
                    r.push(Dynamic {
                        dhtype: x,
                        content: DynamicContent::Address(val),
                    });
                }
            };
        }

        Ok(SectionContent::Dynamic(r))
    }
}
