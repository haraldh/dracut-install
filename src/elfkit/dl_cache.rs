pub const CACHEMAGIC: &'static [u8; 11usize] = b"ld.so-1.7.0";
pub const CACHEMAGIC_NEW: &'static [u8; 17usize] = b"glibc-ld.so.cache";
pub const CACHE_VERSION: &'static [u8; 3usize] = b"1.1";
//pub const CACHEMAGIC_VERSION_NEW: &'static [u8; 20usize] = b"glibc-ld.so.cache1.1";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct FileEntry {
    pub flags: ::std::os::raw::c_int,
    pub key: ::std::os::raw::c_uint,
    pub value: ::std::os::raw::c_uint,
}

#[test]
fn bindgen_test_layout_file_entry() {
    assert_eq!(
        ::std::mem::size_of::<FileEntry>(),
        12usize,
        concat!("Size of: ", stringify!(file_entry))
    );
    assert_eq!(
        ::std::mem::align_of::<FileEntry>(),
        4usize,
        concat!("Alignment of ", stringify!(file_entry))
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<FileEntry>())).flags as *const _ as usize },
        0usize,
        concat!(
            "Offset of field: ",
            stringify!(file_entry),
            "::",
            stringify!(flags)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<FileEntry>())).key as *const _ as usize },
        4usize,
        concat!(
            "Offset of field: ",
            stringify!(file_entry),
            "::",
            stringify!(key)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<FileEntry>())).value as *const _ as usize },
        8usize,
        concat!(
            "Offset of field: ",
            stringify!(file_entry),
            "::",
            stringify!(value)
        )
    );
}

#[repr(C)]
#[derive(Debug)]
pub struct CacheFile {
    pub magic: [u8; 11usize],
    pub nlibs: ::std::os::raw::c_uint,
}

#[test]
fn bindgen_test_layout_cache_file() {
    assert_eq!(
        ::std::mem::size_of::<CacheFile>(),
        16usize,
        concat!("Size of: ", stringify!(cache_file))
    );
    assert_eq!(
        ::std::mem::align_of::<CacheFile>(),
        4usize,
        concat!("Alignment of ", stringify!(cache_file))
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<CacheFile>())).magic as *const _ as usize },
        0usize,
        concat!(
            "Offset of field: ",
            stringify!(cache_file),
            "::",
            stringify!(magic)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<CacheFile>())).nlibs as *const _ as usize },
        12usize,
        concat!(
            "Offset of field: ",
            stringify!(cache_file),
            "::",
            stringify!(nlibs)
        )
    );
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct FileEntryNew {
    pub flags: i32,
    pub key: u32,
    pub value: u32,
    pub osversion: u32,
    pub hwcap: u64,
}

#[test]
fn bindgen_test_layout_file_entry_new() {
    assert_eq!(
        ::std::mem::size_of::<FileEntryNew>(),
        24usize,
        concat!("Size of: ", stringify!(file_entry_new))
    );
    assert_eq!(
        ::std::mem::align_of::<FileEntryNew>(),
        8usize,
        concat!("Alignment of ", stringify!(file_entry_new))
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<FileEntryNew>())).flags as *const _ as usize },
        0usize,
        concat!(
            "Offset of field: ",
            stringify!(file_entry_new),
            "::",
            stringify!(flags)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<FileEntryNew>())).key as *const _ as usize },
        4usize,
        concat!(
            "Offset of field: ",
            stringify!(file_entry_new),
            "::",
            stringify!(key)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<FileEntryNew>())).value as *const _ as usize },
        8usize,
        concat!(
            "Offset of field: ",
            stringify!(file_entry_new),
            "::",
            stringify!(value)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<FileEntryNew>())).osversion as *const _ as usize },
        12usize,
        concat!(
            "Offset of field: ",
            stringify!(file_entry_new),
            "::",
            stringify!(osversion)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<FileEntryNew>())).hwcap as *const _ as usize },
        16usize,
        concat!(
            "Offset of field: ",
            stringify!(file_entry_new),
            "::",
            stringify!(hwcap)
        )
    );
}

#[repr(C)]
#[derive(Debug)]
pub struct CacheFileNew {
    pub magic: [u8; 17usize],
    pub version: [u8; 3usize],
    pub nlibs: u32,
    pub len_strings: u32,
    pub unused: [u32; 5usize],
}

#[test]
fn bindgen_test_layout_cache_file_new() {
    assert_eq!(
        ::std::mem::size_of::<CacheFileNew>(),
        48usize,
        concat!("Size of: ", stringify!(cache_file_new))
    );
    assert_eq!(
        ::std::mem::align_of::<CacheFileNew>(),
        8usize,
        concat!("Alignment of ", stringify!(cache_file_new))
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<CacheFileNew>())).magic as *const _ as usize },
        0usize,
        concat!(
            "Offset of field: ",
            stringify!(cache_file_new),
            "::",
            stringify!(magic)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<CacheFileNew>())).version as *const _ as usize },
        17usize,
        concat!(
            "Offset of field: ",
            stringify!(cache_file_new),
            "::",
            stringify!(version)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<CacheFileNew>())).nlibs as *const _ as usize },
        20usize,
        concat!(
            "Offset of field: ",
            stringify!(cache_file_new),
            "::",
            stringify!(nlibs)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<CacheFileNew>())).len_strings as *const _ as usize },
        24usize,
        concat!(
            "Offset of field: ",
            stringify!(cache_file_new),
            "::",
            stringify!(len_strings)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<CacheFileNew>())).unused as *const _ as usize },
        28usize,
        concat!(
            "Offset of field: ",
            stringify!(cache_file_new),
            "::",
            stringify!(unused)
        )
    );
}
