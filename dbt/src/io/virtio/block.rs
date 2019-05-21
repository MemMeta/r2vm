use super::{Device, DeviceId, Queue};

#[allow(dead_code)]
const VIRTIO_BLK_F_RO: usize = 5;

const VIRTIO_BLK_T_IN  : u32 = 0;
const VIRTIO_BLK_T_OUT : u32 = 1;
// TODO: This is an un-documented but required feature yet to support.
#[allow(dead_code)]
const VIRTIO_BLK_T_GET_ID : u32 = 8;

#[repr(C)]
struct VirtioBlkReqHeader {
    r#type: u32,
    reserved: u32,
    sector: u64,
}

use std::fs::File;
use std::io::{Seek, Read, Write};

pub struct Block {
    status: u32,
    queue: Queue,
    config: [u8; 8],
    file: File,
}

impl Block {
    pub fn new(path: impl AsRef<std::path::Path>) -> Block {
        let file = std::fs::OpenOptions::new()
                                        .read(true)
                                        .write(true)
                                        .open(path)
                                        .unwrap();
        let len = file.metadata().unwrap().len();
        if len % 512 != 0 {
            panic!("Size of the backing file of block device must be multiple of 512 bytes");
        }
        Block {
            status: 0,
            queue: Queue::new(),
            config: (len / 512).to_le_bytes(),
            file,
        }
    }
}

impl Device for Block {
    fn device_id(&self) -> DeviceId { DeviceId::Block }
    fn device_feature(&self) -> u32 { 0 }
    fn driver_feature(&mut self, _value: u32) {}
    fn get_status(&self) -> u32 { self.status }
    fn set_status(&mut self, status: u32) { self.status = status }
    fn config_space(&self) -> &[u8] { &self.config }
    fn queues(&mut self) -> &mut [Queue] {
        std::slice::from_mut(&mut self.queue)
    }
    fn reset(&mut self) {
        self.status = 0;
        self.queue.reset();
    }
    fn notify(&mut self, _idx: usize) {
        while let Some(mut buffer) = self.queue.take() {
            let header: VirtioBlkReqHeader = unsafe {
                let mut header: [u8; 16] = std::mem::uninitialized();
                buffer.read(0, &mut header);
                std::mem::transmute(header)
            };

            match header.r#type {
                VIRTIO_BLK_T_IN => {
                    self.file.seek(std::io::SeekFrom::Start(header.sector * 512)).unwrap();

                    let mut io_buffer = Vec::with_capacity(buffer.write_len());
                    unsafe { io_buffer.set_len(io_buffer.capacity() - 1) };
                    self.file.read_exact(&mut io_buffer).unwrap();
                    trace!(target: "VirtioBlk", "read {} bytes from sector {:x}", io_buffer.len(), header.sector);

                    io_buffer.push(0);
                    buffer.write(0, &io_buffer);
                }
                VIRTIO_BLK_T_OUT => {
                    self.file.seek(std::io::SeekFrom::Start(header.sector * 512)).unwrap();

                    let mut io_buffer = Vec::with_capacity(buffer.read_len() - 16);
                    unsafe { io_buffer.set_len(io_buffer.capacity()) };
                    buffer.read(16, &mut io_buffer);

                    self.file.write_all(&io_buffer).unwrap();
                    // We must make sure the data has been flushed into the disk before returning
                    self.file.sync_data().unwrap();
                    trace!(target: "VirtioBlk", "write {} bytes from sector {:x}", io_buffer.len(), header.sector);

                    buffer.write(0, &[0]);
                }
                _ => {
                    error!(target: "VirtioBlk", "unsupported block operation type {}", header.r#type);
                    continue
                }
            }

            unsafe { self.queue.put(buffer); }
        }

        // TODO
        unsafe { crate::emu::PLIC.as_mut().unwrap().trigger(1) };
    }

}
