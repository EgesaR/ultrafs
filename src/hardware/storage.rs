use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::fs::OpenOptions;
use std::io::{Read, Result, Seek, SeekFrom, Write}; // Added Read
use std::os::unix::fs::OpenOptionsExt;
use std::ptr::NonNull;

pub const BLOCK_SIZE: usize = 4096;
const MAGIC_SIGNATURE: &[u8; 4] = b"UFS!";

pub struct AlignedBlock {
    ptr: NonNull<u8>,
    layout: Layout,
}

unsafe impl Send for AlignedBlock {}

impl AlignedBlock {
    pub fn new() -> Self {
        let layout = Layout::from_size_align(BLOCK_SIZE, BLOCK_SIZE).unwrap();
        let ptr = unsafe { alloc_zeroed(layout) };
        Self {
            ptr: NonNull::new(ptr).expect("Failed to allocate aligned memory"),
            layout,
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), BLOCK_SIZE) }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), BLOCK_SIZE) }
    }
}

impl Drop for AlignedBlock {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

pub struct UltraStorage {
    pub device: std::fs::File,
}

impl UltraStorage {
    pub fn open(path: &str) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .custom_flags(libc::O_DIRECT) 
            .open(path)?;
        
        Ok(Self { device: file })
    }

    pub fn format_superblock(&mut self) -> Result<()> {
        let mut block = AlignedBlock::new();
        let buffer = block.as_mut_slice();

        buffer[0..4].copy_from_slice(MAGIC_SIGNATURE);
        buffer[4..6].copy_from_slice(&0x0100_u16.to_le_bytes());
        buffer[6..10].copy_from_slice(&(BLOCK_SIZE as u32).to_le_bytes());
        buffer[10..18].copy_from_slice(&2_097_152_u64.to_le_bytes());

        self.device.seek(SeekFrom::Start(0))?;
        self.device.write_all(block.as_slice())?;
        self.device.sync_all()?;

        Ok(())
    }

    // NEW: Needed so main.rs can read the DAG out of Sector 0
    pub fn read_block(&mut self, block_index: u64, block: &mut AlignedBlock) -> Result<()> {
        let offset = block_index * (BLOCK_SIZE as u64);
        self.device.seek(SeekFrom::Start(offset))?;
        self.device.read_exact(block.as_mut_slice())?;
        Ok(())
    }

    pub fn write_block(&mut self, block_index: u64, block: &AlignedBlock) -> Result<()> {
        let offset = block_index * (BLOCK_SIZE as u64);
        self.device.seek(SeekFrom::Start(offset))?;
        self.device.write_all(block.as_slice())?;
        Ok(())
    }
}